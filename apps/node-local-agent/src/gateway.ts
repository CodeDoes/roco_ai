import * as http from 'http';

export class RocoGateway {
  private server: http.Server;
  private port: number;
  private targetPort: number;

  constructor(port = 8000, targetPort = 8080) {
    this.port = port;
    this.targetPort = targetPort;
    this.server = http.createServer((req, res) => this.handleProxy(req, res));
  }

  start(): Promise<void> {
    return new Promise((resolve) => {
      this.server.listen(this.port, () => {
        resolve();
      });
    });
  }

  stop(): Promise<void> {
    return new Promise((resolve, reject) => {
      this.server.close((err) => {
        if (err) reject(err);
        else resolve();
      });
    });
  }

  private handleProxy(req: http.IncomingMessage, res: http.ServerResponse) {
    res.setHeader('X-RateLimit-Limit', '100');
    res.setHeader('X-RateLimit-Remaining', '99');

    const options = {
      hostname: 'localhost',
      port: this.targetPort,
      path: req.url,
      method: req.method,
      headers: req.headers,
    };

    const proxyReq = http.request(options, (proxyRes) => {
      res.writeHead(proxyRes.statusCode || 200, proxyRes.headers);
      proxyRes.pipe(res, { end: true });
    });

    req.pipe(proxyReq, { end: true });

    proxyReq.on('error', (err) => {
      res.writeHead(502, { 'Content-Type': 'text/plain' });
      res.end(`Bad Gateway: ${err.message}`);
    });
  }
}
