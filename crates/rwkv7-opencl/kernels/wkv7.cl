/*
 * RWKV-7 WKV Kernel in OpenCL 3.0
 * 
 * High-performance implementation targeting:
 * - AMD RDNA/RDNA2/RDNA3 (via ROCm or AMDGPU-PRO)
 * - NVIDIA (via NVIDIA OpenCL)
 * - Intel (via NEO or Compute Runtime)
 * - Any OpenCL 3.0+ device
 *
 * Optimizations:
 * 1. Subgroup operations (like CUDA warps)
 * 2. Vectorized loads (float4)
 * 3. Local memory for state
 * 4. Loop unrolling
 * 5. FMA operations
 */

#pragma OPENCL cl_khr_fp16
#pragma OPENCL cl_khr_subgroup_shuffle
#pragma OPENCL cl_khr_subgroup_reduce

#define HEAD_SIZE 64
#define LOCAL_SIZE 64

// Constants for w_delta rotator
#define TWO_NEG_41 4.547473508864641e-13
#define NEXP_HALF_LOG2_E -0.8750387749145276
#define NLOG2_E -1.4426950408889634
#define ROT1 2654435769u

// Rotator for numerical stability
inline float rotator1(int x) {
    uint bits = ROT1 * (uint)x;
    return TWO_NEG_41 * (float)(int)bits;
}

// Compute decay delta from weight
inline float w_delta(float w, int phase) {
    float d = exp2(NEXP_HALF_LOG2_E / (1.0f + exp(NLOG2_E * w))) - 1.0f + rotator1(phase);
    return d;
}

// Subgroup reduction sum
inline float subgroup_sum(float val) {
    for (uint offset = get_sub_group_size() / 2; offset > 0; offset >>= 1) {
        val += sub_group_shuffle_down(val, offset);
    }
    return val;
}

/*
 * WKV7 kernel - basic version
 *
 * Grid: (B * H)
 * Workgroup: (LOCAL_SIZE)
 */
__kernel void wkv7_kernel(
    const uint B,
    const uint T,
    const uint C,
    const uint H,
    __global const float* r,
    __global const float* w,
    __global const float* k,
    __global const float* v,
    __global const float* a,
    __global const float* b,
    __global float* state,
    __global float* dst
) {
    const uint batch_id = get_group_id(0) / H;
    const uint head_id = get_group_id(0) % H;
    const uint tid = get_local_id(0);
    
    const uint state_size = C * HEAD_SIZE;
    const uint n_seq_tokens = T / B;
    
    if (batch_id >= B || head_id >= H || tid >= HEAD_SIZE) {
        return;
    }
    
    // Load state into local memory
    __local float state_local[HEAD_SIZE][HEAD_SIZE];
    
    const uint state_offset = batch_id * state_size + head_id * HEAD_SIZE * HEAD_SIZE;
    
    // Each thread loads one row of state
    #pragma unroll
    for (uint j = 0; j < HEAD_SIZE; j++) {
        state_local[tid][j] = state[state_offset + tid * HEAD_SIZE + j];
    }
    barrier(CLK_LOCAL_MEM_FENCE);
    
    // Process each timestep
    const uint start_t = batch_id * n_seq_tokens * C + head_id * HEAD_SIZE + tid;
    
    for (uint t_offset = 0; t_offset < n_seq_tokens; t_offset++) {
        const uint t = start_t + t_offset * C;
        
        // Load inputs into registers
        float r_val = r[t];
        float w_val = w[t];
        float k_val = k[t];
        float v_val = v[t];
        float a_val = a[t];
        float b_val = b[t];
        
        barrier(CLK_LOCAL_MEM_FENCE);
        
        // Copy to local for reduction
        __local float a_local[HEAD_SIZE];
        __local float r_local[HEAD_SIZE];
        a_local[tid] = a_val;
        r_local[tid] = r_val;
        barrier(CLK_LOCAL_MEM_FENCE);
        
        // Compute sa = dot(a, state[tid])
        float sa = 0.0f;
        #pragma unroll
        for (uint j = 0; j < HEAD_SIZE; j++) {
            sa += a_local[j] * state_local[tid][j];
        }
        
        // State update: state[tid][j] = state[tid][j] * w[j] + v[tid] * k[j] + sa * b[j]
        // But since state is stored transposed, we update state[j][tid]
        #pragma unroll
        for (uint j = 0; j < HEAD_SIZE; j++) {
            float w_j = w[t - tid + j];  // Load w for column j
            float k_j = k[t - tid + j];
            float b_j = b[t - tid + j];
            
            state_local[j][tid] = state_local[j][tid] * w_j + v_val * k_j + sa * b_j;
        }
        
        barrier(CLK_LOCAL_MEM_FENCE);
        
        // Compute output: y[tid] = dot(state[tid], r)
        float y = 0.0f;
        #pragma unroll
        for (uint j = 0; j < HEAD_SIZE; j++) {
            y += state_local[tid][j] * r_local[j];
        }
        
        dst[t] = y;
    }
    
    // Store state back
    #pragma unroll
    for (uint j = 0; j < HEAD_SIZE; j++) {
        state[state_offset + tid * HEAD_SIZE + j] = state_local[tid][j];
    }
}

/*
 * WKV7 kernel - optimized with w_delta and vectorized loads
 */
__kernel void wkv7_kernel_opt(
    const uint B,
    const uint T,
    const uint C,
    const uint H,
    __global const float* r,
    __global const float* w,
    __global const float* w0,
    __global const float* k,
    __global const float* v,
    __global const float* a,
    __global const float* b,
    __global const float* kk,
    __global float* state,
    __global float* dst,
    const uint elapsed_t
) {
    const uint batch_id = get_group_id(0) / H;
    const uint head_id = get_group_id(0) % H;
    const uint tid = get_local_id(0);
    const uint subgroup_local_id = get_sub_group_local_id();
    
    const uint state_size = C * HEAD_SIZE;
    const uint n_seq_tokens = T / B;
    
    if (batch_id >= B || head_id >= H || tid >= HEAD_SIZE) {
        return;
    }
    
    // Load state
    __local float state_local[HEAD_SIZE][HEAD_SIZE];
    const uint state_offset = batch_id * state_size + head_id * HEAD_SIZE * HEAD_SIZE;
    
    // Vectorized load using float4
    const uint state_vec_offset = state_offset + tid * HEAD_SIZE;
    #pragma unroll
    for (uint j = 0; j < HEAD_SIZE; j += 4) {
        float4 state_vec = vload4(0, &state[state_vec_offset + j]);
        state_local[tid][j]   = state_vec.x;
        state_local[tid][j+1] = state_vec.y;
        state_local[tid][j+2] = state_vec.z;
        state_local[tid][j+3] = state_vec.w;
    }
    barrier(CLK_LOCAL_MEM_FENCE);
    
    const uint start_t = batch_id * n_seq_tokens * C + head_id * HEAD_SIZE + tid;
    const uint channel_offset = head_id * HEAD_SIZE;
    
    for (uint t_offset = 0; t_offset < n_seq_tokens; t_offset++) {
        const uint t = start_t + t_offset * C;
        
        // Load inputs
        float r_val = r[t];
        float w_raw = w[t];
        float k_val = k[t];
        float v_val = v[t];
        float a_raw = a[t];
        float b_val = b[t];
        float kk_val = kk[t];
        
        // Compute w_delta
        int phase = elapsed_t + t_offset * HEAD_SIZE + tid;
        float w_val = w_delta(w_raw + w0[channel_offset + tid], phase);
        
        // Compute normalized a
        float a_val = a_raw;
        float kka = kk_val * a_val;
        
        // Compute sa using subgroup reduction
        __local float a_local[HEAD_SIZE];
        a_local[tid] = a_val;
        barrier(CLK_LOCAL_MEM_FENCE);
        
        float sa = 0.0f;
        #pragma unroll
        for (uint j = 0; j < HEAD_SIZE; j++) {
            sa += a_local[j] * state_local[tid][j];
        }
        
        // State update with decay
        #pragma unroll
        for (uint j = 0; j < HEAD_SIZE; j++) {
            float w_j = w[t - tid + j] + w0[channel_offset + j];
            float w_delta_j = w_delta(w_j, elapsed_t + t_offset * HEAD_SIZE + j);
            float k_j = k[t - tid + j];
            float b_j = b[t - tid + j];
            float neg_kk_j = -kk[t - tid + j];
            
            state_local[j][tid] = state_local[j][tid] * (1.0f - w_delta_j) 
                                + v_val * k_j 
                                + sa * b_j
                                + state_local[j][tid] * kka * neg_kk_j;
        }
        
        barrier(CLK_LOCAL_MEM_FENCE);
        
        // Compute output
        float y = 0.0f;
        #pragma unroll
        for (uint j = 0; j < HEAD_SIZE; j++) {
            y += state_local[tid][j] * r[t - tid + j];
        }
        
        dst[t] = y;
    }
    
    // Store state with vectorized store
    const uint state_vec_offset = state_offset + tid * HEAD_SIZE;
    #pragma unroll
    for (uint j = 0; j < HEAD_SIZE; j += 4) {
        float4 state_vec = {
            state_local[tid][j],
            state_local[tid][j+1],
            state_local[tid][j+2],
            state_local[tid][j+3]
        };
        vstore4(state_vec, 0, &state[state_vec_offset + j]);
    }
}

/*
 * Layer Normalization kernel
 */
__kernel void layer_norm_kernel(
    const uint C,
    const uint T,
    const uint B,
    __global const float* weight,
    __global const float* bias,
    __global float* x,
    const float eps
) {
    const uint idx = get_global_id(0);
    const uint token = get_global_id(1);
    const uint batch = get_global_id(2);
    
    const uint stride = C / 4;
    const uint bb = (batch * T + token) * stride;
    
    if (idx >= stride) return;
    
    // Parallel reduction for mean
    float4 sum = (float4)(0.0f);
    float4 m2 = (float4)(0.0f);
    uint count = 0;
    
    for (uint i = idx; i < stride; i += LOCAL_SIZE) {
        float4 value = vload4(0, &x[bb + i]);
        float4 delta = value - sum;
        count++;
        sum += delta / (float)count;
        m2 += delta * (value - sum);
    }
    
    // Subgroup reduction
    float sum_scalar = sum.x + sum.y + sum.z + sum.w;
    sum_scalar = subgroup_sum(sum_scalar);
    
    float m2_scalar = m2.x + m2.y + m2.z + m2.w;
    m2_scalar = subgroup_sum(m2_scalar);
    
    // Compute mean and std
    float mean = sum_scalar / (float)C;
    float var = m2_scalar / (float)C + eps;
    float inv_std = rsqrt(var);
    
    // Normalize and apply affine
    for (uint i = idx; i < stride; i += LOCAL_SIZE) {
        float4 value = vload4(0, &x[bb + i]);
        float4 w = vload4(0, &weight[i * 4]);
        float4 b = vload4(0, &bias[i * 4]);
        
        float4 normalized = (value - mean) * inv_std;
        vstore4(fma(normalized, w, b), 0, &x[bb + i]);
    }
}
