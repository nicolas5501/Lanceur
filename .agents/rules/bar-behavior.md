# Spécification : Comportement du Bandeau et du Focus

## Le Focus (Priorité absolue)

- **Zéro vol de focus** : Le bandeau ne doit **jamais** voler le focus clavier/fenêtre active de l'application en cours.
- Cette règle s'applique dans tous les cas :
  - Lors de l'**apparition** du bandeau (raccourci clavier, clic systray)
  - Lors des **clics souris** sur le bandeau (ouverture d'un menu dropdown, lancement d'un item)
- L'application active de l'utilisateur doit toujours rester au premier plan.

### Implémentation technique obligatoire
- `WM_MOUSEACTIVATE` → toujours retourner `MA_NOACTIVATE`
- `ShowWindow` → utiliser `SW_SHOWNOACTIVATE`
- `SetWindowPos` → toujours inclure le flag `SWP_NOACTIVATE`
- Style étendu : `WS_EX_NOACTIVATE` doit rester présent en permanence

## Le Bandeau (L'interface)

- **Pleine largeur** : Le bandeau doit occuper 100% de la largeur de l'écran (modes Top et Bottom).
  - Le `width: 100%` doit être présent sur le rectangle principal dans `bar.slint`.
  - `position_bar_window` doit toujours utiliser `work_w` (largeur réelle de la zone de travail).
- **Invisible pour Windows** :
  - Pas d'icône dans la barre des tâches : style `WS_EX_TOOLWINDOW`, pas de `WS_EX_APPWINDOW`.
  - Absent du menu `Alt+Tab`.
- **Résistance à Win+D** : Le bandeau ne doit pas se minimiser avec `Win+D`.
  - Intercepter `SC_MINIMIZE`, `WM_WINDOWPOSCHANGING` (flag `SWP_HIDEWINDOW`), `WM_SIZE` (state minimized), `WM_SHOWWINDOW` (hide forced).
  - Ne jamais agir si `BAR_EXPLICITLY_HIDDEN` est `true`.
- **Clic droit** : Un clic droit sur **n'importe quelle zone** du bandeau doit afficher le menu contextuel natif.
  - Intercepter `WM_RBUTTONUP | WM_CONTEXTMENU` dans le subclass proc.
  - La `TouchArea` Slint de fond (`bar_bg_touch`) doit couvrir l'intégralité du `main_bar`.

## Initialisation

- Au lancement, la fenêtre doit être **positionnée hors écran** (`x: -30000`) avant même que Slint ne la rende visible.
- Une fois le HWND obtenu, appliquer immédiatement les styles Win32 et la bonne taille/position avec `position_bar_window`.
- Seulement après, appeler `hide_bar_window()` pour la renvoyer dans le systray.
- Ne **jamais** utiliser `SW_HIDE` lors de l'initialisation : cela fermerait la boucle d'événements de Slint.
