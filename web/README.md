# React + TypeScript + Vite

This template provides a minimal setup to get React working in Vite with HMR and some Oxlint rules.

Currently, two official plugins are available:

- [@vitejs/plugin-react](https://github.com/vitejs/vite-plugin-react/blob/main/packages/plugin-react) uses [Oxc](https://oxc.rs)
- [@vitejs/plugin-react-swc](https://github.com/vitejs/vite-plugin-react/blob/main/packages/plugin-react-swc) uses [SWC](https://swc.rs/)

## React Compiler

The React Compiler is not enabled on this template because of its impact on dev & build performances. To add it, see [this documentation](https://react.dev/learn/react-compiler/installation).

## Expanding the Oxlint configuration

If you are developing a production application, we recommend enabling type-aware lint rules by installing `oxlint-tsgolint` and editing `.oxlintrc.json`:

```json
{
  "$schema": "./node_modules/oxlint/configuration_schema.json",
  "plugins": ["react", "typescript", "oxc"],
  "options": {
    "typeAware": true
  },
  "rules": {
    "react/rules-of-hooks": "error",
    "react/only-export-components": ["warn", { "allowConstantExport": true }]
  }
}
```

See the [Oxlint rules documentation](https://oxc.rs/docs/guide/usage/linter/rules) for the full list of rules and categories.

## Variables d'environnement de Nestor

À placer dans `web/.env.local` ou devant la commande `vite` :

- `VITE_NESTOR_WS_URL` : adresse du WebSocket du daemon, à la place de la
  valeur par défaut : `wss://<hôte de la page>/ws` quand la page est servie en
  HTTPS (Tailscale, cf. `../docs/tailscale.md`), sinon `ws://127.0.0.1:8340/ws`. Les points d'accès `/auth/*` sont appelés sur le même
  hôte. Exemple, pour pointer l'interface sur un daemon de test :
  `VITE_NESTOR_WS_URL=ws://127.0.0.1:8351/ws npx vite --port 5183`.
- `VITE_NESTOR_TOKEN` : jeton d'accès fourni à la compilation (développement).
