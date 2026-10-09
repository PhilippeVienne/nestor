# Santé de l'installation

Nestor ne sert que si ce qui l'entoure tourne. Le daemon vérifie toutes les
cinq minutes (`nestord/src/health.rs`), publie l'événement `health` et le
panneau « Santé » de l'interface l'affiche ; un élément hors service devient
une alerte de la boucle proactive, dite à la voix, une fois jusqu'au retour.

| Élément | Vérification | Seuils |
|---|---|---|
| Session claude | processus lancé | hors service si absent (relance en cours) |
| Juge (Ollama) | `GET /api/tags` sur `judge.ollama_host` | hors service si injoignable |
| Accès par le tailnet | `GET /auth/status` par le nom public, résolu vers l'adresse Tailscale | hors service si pas de réponse ou pas l'API (page publique) |
| Certificats | `openssl s_client` sur chaque nom de `cert_names` | avertissement à moins de 14 jours, hors service à moins de 3 |
| Sauvegarde | horodatage `~/.local/state/nestord/last-backup` | avertissement au-delà de 2 jours, hors service au-delà de 7 ou absent |

```toml
[health]
enabled = true
interval_minutes = 5
edge_url = "https://nestor.vienne.me"
edge_addr = "100.84.235.85:443"
cert_names = ["nestor.vienne.me", "kanto.felis-ionian.ts.net"]
backup_stamp = "~/.local/state/nestord/last-backup"
```
