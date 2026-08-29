---
trigger: always_on
description: Règles de gestion de version et de synchronisation GitHub pour le projet.
---

# Règles de Synchronisation GitHub

1. **Clean Workspace** : Toujours vérifier que `.gitignore` exclut les répertoires de compilation (`/target/`), les binaires (`*.exe`), ainsi que les caches locaux (`icon_cache/`, `launcher_config.json`).
2. **Atomic Commits** : Faire des commits clairs et structurés avec des messages conventionnels (`feat:`, `fix:`, `style:`, `docs:`, `chore:`).
3. **No Automatic Compile on Sync** : Ne pas lancer de build complet lors des synchronisations Git à moins d'une demande explicite de l'utilisateur.
