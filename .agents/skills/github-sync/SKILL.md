---
name: github-sync
description: Synchronisation et gestion de version du projet avec GitHub (commit, push, pull, branches, tags et releases).
---

# GitHub Synchronization Skill for Antigravity

Ce skill permet à l'agent et à l'utilisateur de gérer facilement le cycle de vie du code et sa synchronisation avec GitHub.

## 1. Commandes de Synchronisation Rapide

### A. Première initialisation et liaison avec GitHub
```powershell
# Initialiser le dépôt local s'il ne l'est pas
git init -b main

# Ajouter l'origine distante (remplacer USER et REPO)
git remote add origin https://github.com/VOTRE_UTILISATEUR/VOTRE_DEPOT.git

# Ajouter les fichiers et premier commit
git add .
git commit -m "feat: initial commit - Launcher Bar V3.2"

# Pousser sur GitHub
git push -u origin main
```

### B. Synchronisation quotidienne (Commit & Push)
```powershell
# Vérifier les fichiers modifiés
git status

# Ajouter toutes les modifications
git add .

# Créer un commit conventionnel
git commit -m "feat: ajout de la personnalisation des couleurs et ergonomie"

# Envoyer vers GitHub
git push origin main
```

### C. Récupération des dernières modifications (Pull)
```powershell
git pull origin main
```

## 2. Conventions de Commit Recommandées

- `feat:` Nouvelle fonctionnalité ajoutée
- `fix:` Correction d'un bug ou d'un comportement
- `refactor:` Refactorisation sans changement fonctionnel
- `style:` Ajustements visuels et UI
- `docs:` Documentation (README, etc.)
- `chore:` Tâches de maintenance, dépendances Cargo, etc.

## 3. Utilisation de GitHub CLI (`gh`)

Si GitHub CLI est installé :
```powershell
# Créer directement le dépôt distant depuis la ligne de commande
gh repo create Lanceur --public --source=. --remote=origin --push

# Vérifier l'état du dépôt
gh repo view
```
