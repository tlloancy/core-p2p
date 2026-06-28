# core-p2p

Bibliothèque iroh pour le seeding et le fetch P2P de blobs vidéo hashés SHA256.

- **Native** — addon Node (`index.node`) pour le gateway serveur
- **WASM** — peer relay-only dans le navigateur (`create_peer_wasm`, `fetch_chunk_wasm`)

## Build native (serveur)

```bash
bash build.sh
```

## Build WASM (navigateur)

Prérequis : `rustup target add wasm32-unknown-unknown`, `clang`, `wasm-pack`.

```bash
bash scripts/build-wasm.sh
```

Produit `pkg/` (target **web**) et copie vers `../sport-app/public/core-p2p/`.

Le navigateur charge le module depuis `/core-p2p/` (pas via npm). Si le WASM est absent, l’app retombe sur `/api/p2p/chunk` (phase 1).

Variable côté client : `NEXT_PUBLIC_P2P_WASM_DISABLED=1` pour forcer le fallback HTTP.

## Protocole blob

- ALPN : `sport-p2p/blob/0`
- Requête : 64 caractères hex SHA256 sur un bi-stream
- Réponse : bytes `.ts`, vérifiés côté client

## Licence

Ce projet est distribué sous licence [AGPL-3.0](LICENSE).
