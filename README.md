# ⚡ Lanceur d'Applications Bandeau & Systray (Rust + Slint)

Un bandeau horizontal fin ultra-performant et modulaire pour Windows, persistant sur le bureau (`Win + D`), résidant dans le Systray avec raccourci global, Drag & Drop natif et gestion complète de conteneurs.

---

## ✨ Fonctionnalités Majeures

### 1. 📏 Bandeau Horizontal Fin & Modulaire
- **Format compact** : Affichage épuré de chaque item avec `[Icône] + [Nom]`.
- **Dimensions & Textes personnalisables** :
  - Hauteur totale du bandeau paramétrable.
  - Hauteur commune des items paramétrable.
  - Taille de police personnalisable séparément pour les conteneurs et les items.
- **Positionnement au choix** :
  - Ancré en **Haut de l'écran** (`Top`).
  - Ancré en **Bas de l'écran** (`Bottom`, au-dessus de la barre des tâches).
  - Mode **Flottant** (`Floating`).

### 2. 🛡️ Résistance à `Win + D` (Show Desktop) & Always on Top
- Intégration des styles Win32 `WS_EX_TOOLWINDOW` et `WS_EX_TOPMOST`.
- La barre ne se minimise pas lors de l'appui sur `Windows + D` et reste immédiatement accessible.

### 3. 🔔 Intégration Systray (Zone de notification)
- Icône résidente dans la barre des tâches système.
- Clic gauche : Bascule instantanée de visibilité (Afficher / Masquer).
- Clic droit : Menu contextuel (`Afficher/Masquer`, `⚙️ Paramètres...`, `✓ Lancer au démarrage`, `❌ Quitter`).

### 4. ⌨️ Raccourci Clavier Global (HotKey)
- Raccourci global paramétrable (ex: `Ctrl + Space`, `Alt + F1`, etc.) pour afficher/masquer le lanceur depuis n'importe quelle application sans quitter sa session de travail.

### 5. 🖱️ Drag & Drop Natif (Glisser-Déposer)
- Glissez-déposez n'importe quel fichier `.exe`, raccourci ou dossier depuis **l'Explorateur Windows** ou le **Bureau** directement sur le bandeau : l'item est instantanément créé avec son nom et son icône extraite.

### 6. 🖼️ Double Système d'Icônes
- **Emojis** : Sélection directe d'emojis ultra-légers (ex: 🚀, 📁, 🌐, ⚡).
- **Icônes réelles extraites** : Extraction automatique et mise en cache haute résolution des icônes depuis les fichiers `.exe`, `.ico` ou `.lnk`.

### 7. ⚙️ Fenêtre de Paramètres Dédiée
- **Gestion des Conteneurs** :
  - Création, suppression, renommage et choix de l'icône de conteneur.
  - Réorganisation de la position dans le bandeau (boutons ⬆️ / ⬇️).
- **Gestion des Items** :
  - Création / modification (nom, cible avec sélecteur de fichier, icône).
  - Réorganisation dans le conteneur (⬆️ / ⬇️).
  - Déplacement d'un conteneur à un autre.
  - Duplication d'items (dans le même conteneur ou vers un conteneur cible).
- **Préférences Système & Ergonomie** :
  - Démarrage automatique avec Windows (via le Registre).
  - Réglage des dimensions, polices, raccourcis et positionnement.

---

## 🚀 Compilation & Exécution

- **Exécuter en mode développement :**
  ```powershell
  cargo run
  ```

- **Compiler en mode Release (optimisé à ~5 Mo de RAM) :**
  ```powershell
  cargo build --release
  ```
  *(L'exécutable autonome se trouve dans `target/release/lanceur.exe`)*

