//! Desktop Pet Demo — conversational companion.
//!
//! Run: cargo run --example pet_demo -p roco_ui
//!
//! Transparent, always-on-top, draggable. Click the face to open chat.
//! Type a message and press Enter to get a response from the inference backend.
//! If no backend is available, echoes your message.

use eframe::egui;
use egui::Vec2;
use roco_ui::{pet_native_options, DesktopPet, PetMood};

fn main() -> eframe::Result<()> {
    let options = pet_native_options(Vec2::new(260.0, 360.0));

    eframe::run_native(
        "RoCo Pet",
        options,
        Box::new(|_cc| {
            let mut pet = DesktopPet::default();
            pet.set_status("Click to chat!");

            // Try connecting to the inference backend
            let backend = try_connect_backend();
            if let Some(backend) = backend {
                pet.on_message(move |msg, history| {
                    let history_text: String = history
                        .iter()
                        .map(|m| format!("{}: {}", m.role, m.text))
                        .collect::<Vec<_>>()
                        .join("\n");

                    let system = "\
                        You are a cute desktop pet. Be warm, playful, and conversational.\n\
                        Keep responses short (1-3 sentences). React to what the user says.\n\
                        Use emoticons and be expressive!"
                        .to_string();

                    let prompt = format!("Conversation so far:\n{history_text}\nUser: {msg}\nPet:");

                    let request = roco_engine::CompletionRequest {
                        system,
                        prompt,
                        temperature: 0.8,
                        max_tokens: 256,
                        prefill: Some(" ".into()),
                        ..Default::default()
                    };

                    match futures::executor::block_on(backend.complete(request)) {
                        Ok(resp) => {
                            let text = resp.text.trim().to_string();
                            if text.is_empty() {
                                None
                            } else {
                                Some(text)
                            }
                        }
                        Err(_) => None,
                    }
                });
            } else {
                // No backend — echo mode
                pet.on_message(|msg, _| Some(format!("You said: {msg}")));
            }

            Ok(Box::new(PetApp { pet }))
        }),
    )
}

struct PetApp {
    pet: DesktopPet,
}

impl eframe::App for PetApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.pet.tick(ctx) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        // Repaint rate
        match self.pet.mood {
            PetMood::Sleep => ctx.request_repaint_after(std::time::Duration::from_secs(2)),
            _ => ctx.request_repaint_after(std::time::Duration::from_millis(100)),
        }
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        egui::Color32::TRANSPARENT.to_normalized_gamma_f32()
    }
}

/// Try to connect to the inference backend via the gateway.
fn try_connect_backend() -> Option<std::sync::Arc<dyn roco_engine::ModelBackend>> {
    // Check if gateway is healthy by a quick async request
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .ok()?;
    let ok = rt.block_on(async {
        reqwest::get("http://127.0.0.1:8000/health")
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    });
    if ok {
        Some(std::sync::Arc::new(roco_infer_client::RemoteBackend::new(
            "http://127.0.0.1:8000".to_string(),
        )))
    } else {
        None
    }
}
