# Page publique de nestor.vienne.me

Un Worker Cloudflare, sans origine : depuis Internet, `nestor.vienne.me` ne
montre que cette page. Kanto n'est jamais joignable par là.

```sh
cd deploy/cloudflare-worker
npx wrangler login      # une fois, dans le navigateur
npx wrangler deploy     # cree le Worker et lui attache nestor.vienne.me (domaine personnalise)
```

Le domaine personnalisé crée lui-même l'enregistrement DNS proxifié. Sur le
tailnet, le DNS partagé (`deploy/edge/dnsmasq`) prend le pas et mène à kanto.
