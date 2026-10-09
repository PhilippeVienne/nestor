// Page publique de Nestor : sobre, sans rien revelant de l'installation.
// Meme systeme visuel que l'interface (docs/visuel.md) : encre, ivoire, laiton, serif.
const PAGE = `<!doctype html>
<html lang="fr">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="robots" content="noindex">
<title>Nestor</title>
<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link href="https://fonts.googleapis.com/css2?family=Fraunces:opsz,wght@9..144,300..600&family=Inter:wght@400;500&display=swap" rel="stylesheet">
<style>
  :root { color-scheme: dark; }
  html, body { margin: 0; min-height: 100%; background: #0b0e14; color: #f1ece0; font-family: Inter, system-ui, sans-serif; }
  main { min-height: 100vh; display: grid; place-items: center; padding: 24px 16px; box-sizing: border-box;
         background: radial-gradient(ellipse at top, rgba(207,165,82,.12), transparent 65%); }
  .card { max-width: 420px; width: 100%; border: 1px solid #1c2230; border-radius: 14px; padding: 28px 28px 24px;
          background: linear-gradient(180deg, rgba(28,34,48,.55), rgba(16,20,28,.55)); }
  .orb { width: 56px; height: 56px; border-radius: 50%; margin: 0 auto 18px;
         background: radial-gradient(circle at 38% 36%, #efd9a4 0, #cfa552 45%, #6f5424 100%); box-shadow: 0 0 36px rgba(207,165,82,.25); }
  h1 { font-family: Fraunces, Georgia, serif; font-weight: 500; font-size: 34px; margin: 0; text-align: center; letter-spacing: -.01em; }
  p { margin: 10px 0 0; text-align: center; color: #cdc6b6; line-height: 1.5; font-size: 15px; }
  p.quiet { color: #7a7466; font-size: 13px; margin-top: 18px; }
</style>
</head>
<body>
<main>
  <section class="card">
    <div class="orb" aria-hidden="true"></div>
    <h1>Nestor</h1>
    <p>Majordome personnel, à votre service, discrètement.</p>
    <p class="quiet">Accès réservé. Si vous êtes attendu, passez par la porte de service.</p>
  </section>
</main>
</body>
</html>`;

export default {
  async fetch() {
    return new Response(PAGE, {
      status: 200,
      headers: {
        'content-type': 'text/html; charset=utf-8',
        'cache-control': 'public, max-age=3600',
        'x-robots-tag': 'noindex',
      },
    });
  },
};
