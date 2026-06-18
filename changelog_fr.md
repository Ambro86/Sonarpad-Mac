Journal des nouveautés

Version 0.2.9 - 1er mai 2026
- Les fonctionnalités YouTube ont été étendues également aux Mac Intel et à Catalina.
- La recherche YouTube a été fortement accélérée.
- La gestion des résultats YouTube a été améliorée, avec les chaînes et les playlists placées en tête.
- Ajout de la possibilité d’ajouter et de retirer des chaînes et des playlists des favoris.
- Ajout du bouton Aperçu de la voix dans les options.
- Ajout du bouton Tout sélectionner lors de la suppression des sources.
- Ajout d’une barre de progression pour la recherche Wikipédia.
- Ajout de la chaîne TV Videolina.
- Les entrées de menu des fonctionnalités supplémentaires ont été déplacées dans Outils, afin d’aligner Sonarpad avec la version Windows.
- Correction du comportement où, parfois, les programmes actuellement diffusés à la TV n’étaient pas affichés.
- Ajout de nombreux canaux TV, avec une fenêtre organisée en catégories pour une consultation plus simple. Un champ de recherche a également été ajouté pour afficher les résultats de la TV souhaitée.

Version 0.2.8 - 29 avril 2026
- Ajout du menu Outils avec deux nouvelles entrées : Rechercher et importer depuis Wikipédia et Lire un audio en streaming.
- Rechercher et importer depuis Wikipédia permet de rechercher et d’importer des articles, de les lire et de les enregistrer comme livres audio.
- Lire un audio en streaming permet de lire des contenus en streaming, par exemple depuis YouTube.
- Dans le champ de recherche du streaming, il est possible de saisir n’importe quel contenu : le programme le recherchera et pourra aussi ouvrir des chaînes et des playlists.
- La recherche YouTube n’est pas activée sur les Mac Intel pour des raisons d’incompatibilité.
- Remerciements à Leonardo Graziano pour son soutien continu.
- Pour les radios, un bouton a été ajouté pour aller directement à la page sélectionnée dans les résultats, sans devoir utiliser à chaque fois Aller à la page suivante.
- Le signet automatique a été étendu également aux fichiers texte.
- Correction d’un problème où les audiodescriptions n’étaient parfois pas enregistrées à cause de délais d’attente.
- Ajout de la possibilité de définir des TV favorites.
- Dans la liste des chaînes TV, l’indication du programme actuellement diffusé a été ajoutée.
- Ajout d’un guide TV complet, consultable depuis la veille jusqu’à cinq jours après la date actuelle.

Version 0.2.7 - 28 avril 2026
- Amélioration du support des fichiers avec signes diacritiques et encodages différents de l’UTF-8, y compris les caractères chinois et d’autres langues internationales.
- Correction du problème où la virgule, saisie dans un champ de texte, ouvrait par erreur les options.
- Amélioration de la vitesse de lecture : les articles longs sont désormais lus plus rapidement et la pause après les paragraphes a été supprimée.
- Ajout de la possibilité d’ouvrir avec Sonarpad des fichiers JPG et formats similaires, afin d’effectuer l’OCR aussi sur des articles envoyés comme images ou photos.
- Ajout de la possibilité de définir Sonarpad comme programme par défaut.
- Sonarpad peut désormais ouvrir non seulement des fichiers texte, mais aussi des fichiers audio et vidéo, en utilisant le lecteur MPV.
- Ajout dans les options de la fonction de signet automatique : lorsqu’un fichier, un podcast ou tout contenu multimédia est fermé, il sera rouvert exactement à l’endroit où il avait été laissé.
- Les radios ne sont plus ouvertes dans Safari, mais sont lues directement via le lecteur de Sonarpad.
- À partir de cette version, l’app est signée et ne nécessite plus aucune autorisation de la part de l’utilisateur, ce qui simplifie l’installation.
- Ajout d’une mise à jour automatique du programme, qui vérifie, télécharge et met à jour Sonarpad automatiquement.
- Ajout des modules supplémentaires RaiPlay, Audiodescriptions Rai, RaiPlay Sound et chaînes TV. Pour les utiliser, il faudra demander un code à l’auteur.
- Pour obtenir le code, suivre la procédure indiquée par le programme et envoyer l’e-mail généré, en s’assurant qu’il est bien présent dans les messages envoyés. Si la procédure est correctement effectuée, le code sera reçu en environ une minute.
- Le code doit être saisi en ouvrant les options avec Command + , puis en se déplaçant avec VO + flèche droite jusqu’au champ Code Sonarpad pour fonctionnalités supplémentaires.
- Remarque : si une erreur apparaît lors de l’ouverture d’une fonctionnalité supplémentaire, par exemple RaiPlay, cela signifie probablement que le code n’a pas été copié entièrement.
- Dans les modules Rai, la recherche et la consultation des contenus ont été ajoutées ; ils sont lus via le lecteur de Sonarpad.

Version 0.2.6
- Correction d’un bug wx/macOS qui pouvait afficher une erreur au démarrage et stabilisation des menus associés.
- Correction du raccourci Cmd+, pour le menu Options même lorsque le focus se trouve dans l’éditeur ou sur d’autres contrôles.
- Lors de l’enregistrement d’un livre audio, le focus est maintenant placé correctement dans le champ de texte et les noms de fichiers contenant un point ne sont plus tronqués.
- Ajout du support des OPML de Lire avec division en dossiers : les dossiers s’ouvrent comme sous-menus et les sources individuelles dans une fenêtre dédiée.
- Le réordonnancement des sources d’articles gère maintenant le nouveau système de dossiers avec les boutons Ouvrir le dossier, Dossier principal, Déplacer vers le dossier et Sortir des dossiers.

Version 0.2.5
- Nouvelles fenêtres d’enregistrement personnalisées pour le texte et les livres audio sur macOS.
- Les champs de nom de fichier acceptent maintenant correctement Cmd+V, Cmd+A et les autres commandes d’édition.
- Le programme mémorise le dernier dossier et le dernier format utilisés pour l’enregistrement du texte et des livres audio.
- Ajout de l’enregistrement des livres audio également au format M4A et WAV.
- Ajout du menu Radio avec recherche par langue, ajout aux favoris, ajout manuel d’une station, modification et réorganisation des favoris.
- Amélioration de la gestion des sources d’articles insérées comme sites : découverte du flux depuis la page et correction du flux de commentaires.
- Le flux de publication macOS a été mis à jour pour inclure également l’artefact Catalina.

Version 0.2.4
- Améliorations importantes de l’OCR PDF sur macOS avec le passage à pdfium et des solutions de repli plus robustes.
- Ajout de l’export M4B sur macOS et amélioration de l’enregistrement du texte.
- Amélioration de la gestion des sources d’articles et protection du rafraîchissement lorsqu’une source renvoie zéro élément.
- Optimisation de la synthèse Edge TTS avec découpage et nouvelles tentatives plus fiables.
- Ajout et amélioration du pipeline Catalina pour la compilation et l’empaquetage macOS.

Version 0.2.2
- Amélioration du chargement des PDF sur macOS avec un retour plus clair et une boîte de dialogue finale explicite.
- Tri alphabétique des sources d’articles.
- Corrections du texte PDF et améliorations générales de localisation.

Version 0.2.1
- Stabilisation des raccourcis et menus macOS pour démarrer, mettre en pause, arrêter et enregistrer.
- Amélioration de l’ouverture externe des épisodes de podcast sur macOS.
- Correction de la persistance des options macOS.
- Renforcement des flux de compilation Intel/macOS et de la gestion de Xcode.

Version 0.2.0
- Première version macOS de Sonarpad pour Mac.
- Support de la lecture de texte, d’articles et de podcasts avec synthèse vocale.
- Support de l’OCR PDF sur macOS, téléchargement des mises à jour et paquets DMG dédiés.
- Catégories de podcast hiérarchiques et premiers raccourcis globaux/macOS.
