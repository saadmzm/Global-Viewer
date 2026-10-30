# Global Viewer

Upload and view any file type (images, video, audio, 3D models, PDFs, text) in the browser. Rust (Axum) backend, React frontend, SQLite, local file storage.

## Usage

Requires Rust (stable) and Node.js 20.19+ or 22.12+. Run each part in its own terminal.

### Backend

```bash
cd backend
cargo run
```

### Frontend

npm install is only needed the first time.

```bash
cd frontend
npm install
npm run dev
```

Open <http://localhost:5173> and upload files. Data is stored in `data/`.
