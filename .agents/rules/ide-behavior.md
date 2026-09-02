# Directives de Comportement dans l'EDI

1. **Minimiser l'ouverture d'onglets de fichiers** :
   - Éviter d'ouvrir inutilement des fichiers dans l'éditeur de code de l'utilisateur.
   - Privilégier les outils non intrusifs (`grep_search`, inspection via terminal en arrière-plan) pour lire et analyser le code sans encombrer l'espace de travail de l'utilisateur avec de nouveaux onglets.
   - N'éditer que les fichiers strictement nécessaires pour la tâche demandée.

2. **Compilation systématique après modifications** :
   - À la fin de chaque série de modifications de code, lancer systématiquement la compilation (`cargo build --release`) afin de livrer un binaire directement prêt à être testé et exécuté par l'utilisateur.
