Journal des nouveautés

Version 0.5.0 - 12 septembre 2026

Audiodescription, Outils et conversion par lots

1. Les dernières améliorations du moteur d’audiodescription de Windows ont été portées sur macOS, avec un pont Gemini plus robuste, une meilleure gestion des segments vidéo problématiques et des contrôles plus fiables pendant l’analyse et la réexportation.

2. Dans « Créer une audiodescription avec l’IA », il est désormais possible de choisir entre « Utiliser ma clé API Gemini » et « Utiliser Sonarpad AI ». Les deux identifiants sont conservés séparément : changer de mode ne supprime ni la clé personnelle ni le code Sonarpad AI.

3. Avec Sonarpad AI, le crédit actuel est affiché dans un champ en lecture seule, avec les commandes permettant d’afficher le code et d’en demander un nouveau.

4. Si un fichier contient plusieurs pistes audio, Sonarpad demande quelle piste analyser avant de créer l’audiodescription. La piste choisie est conservée dans le projet et réutilisée lors des opérations suivantes.

5. Ajout d’une option permettant de reconnaître les textes importants affichés à l’écran et d’en tenir compte lors de la génération des descriptions.

6. La création d’une audiodescription peut désormais produire, en option, une vidéo finale contenant l’audiodescription en plus de la sortie audio normale. La gestion des conteneurs et des horodatages a également été améliorée lorsqu’un format de sortie alternatif est nécessaire.

7. Amélioration du ducking de la bande-son originale : la baisse et le retour du volume autour de la narration sont plus progressifs, avec un pré-duck et un relâchement plus doux.

8. Ajout de « Réanalyser le segment » dans l’éditeur de projet d’audiodescription. La commande agit sur la description sélectionnée et utilise automatiquement le dernier mode IA global choisi, comme sous Windows.

9. L’éditeur de projet peut désormais conserver simultanément les modifications de plusieurs descriptions. Les brouillons restent disponibles lors du passage d’un segment à l’autre ou pendant une recherche ; « Appliquer » les valide et les applique ensemble. Si une seule description dépasse le silence disponible, aucune modification n’est appliquée et Sonarpad revient à la description à corriger.

10. Correction du chemin d’enregistrement lors de la création d’une deuxième audiodescription sans fermer la fenêtre : sélectionner un nouveau fichier source génère maintenant un chemin correspondant au nouveau fichier au lieu de conserver l’ancien.

11. Ajout du réglage « Regrouper le menu Outils par catégorie ». Lorsqu’il est activé, Outils est organisé en « Lecture et contenu », « Multimédia » et « Utilitaires » ; lorsqu’il est désactivé, le menu redevient plat.

12. Dans l’encyclopédie Treccani, le contrôle de résultats vide qui apparaissait avant toute recherche a été retiré de l’interface accessible. Il n’est affiché que lorsque des résultats sélectionnables existent.

13. Ajout de « Convertir un dossier… » dans Outils > Multimédia. Un dossier entier peut être converti par lots avec les mêmes formats et paramètres que « Convertir un média », par exemple de nombreux fichiers WMA vers MP3 en une seule opération.

14. La conversion de dossiers affiche la progression fichier par fichier, propose un sous-dossier « Convertis », conserve les noms de base, récapitule les erreurs et protège contre les écrasements accidentels et les collisions de noms de destination.

15. Ajout d’un réglage, activé par défaut, qui annonce à la fois la position actuelle et la durée totale sous une forme naturelle lors d’une avance ou d’un retour dans un média, par exemple « 1 minute et 10 secondes sur 1 heure, 10 minutes et 10 secondes ». Lorsqu’il est désactivé, Sonarpad continue d’annoncer uniquement la position actuelle comme dans les versions précédentes.

16. Pendant la lecture, Option+I annonce uniquement la durée totale du contenu dans un format naturel. Le raccourci fonctionne indépendamment du réglage qui ajoute la durée totale aux annonces lors des déplacements ; pour les flux en direct, Sonarpad annonce qu’il s’agit d’un direct.

17. Dans Créer une audiodescription avec l’IA, le moteur et la voix n’occupent plus la fenêtre principale. Le nouveau bouton « Régler la voix » ouvre une fenêtre dédiée avec moteur, voix, vitesse et volume, ainsi qu’un test de la voix ; les choix sont enregistrés pour les audiodescriptions. Si cette fenêtre n’est jamais utilisée, la vitesse et le volume continuent d’hériter des réglages généraux comme auparavant.

18. « Lire un média en streaming » affiche désormais la durée de chaque vidéo en plus du titre. Dans « Convertir un dossier », les libellés sont plus clairs avec « Choisir le dossier à convertir » et « Dossier de destination » ; pendant une conversion, « Interrompre la conversion » arrête immédiatement le processus FFmpeg actif, supprime le fichier partiel courant et empêche le démarrage des fichiers suivants.

19. Ajout d’une protection de secours pour les sources multicanales problématiques (par exemple 5.1, 6.1 ou 7.1) : le traitement normal reste inchangé et est utilisé comme auparavant ; uniquement si le WAV interne est illisible, présente un format inattendu ou contient des trames PCM mal alignées, Sonarpad régénère automatiquement cette étape en stéréo 48 kHz puis réessaie, afin d’éviter les erreurs de finalisation sans affecter les fichiers qui fonctionnent déjà.

20. Au démarrage d’une conversion de fichier ou de dossier, VoiceOver annonce désormais « Conversion commencée », afin de confirmer immédiatement le lancement sans devoir se déplacer jusqu’à l’indicateur de progression.

21. Fiabilité des sources RSS améliorée : si le flux original d’un média échoue ou ne renvoie aucun article, Sonarpad essaie automatiquement un flux Google News limité au site du même média et dans la langue d’actualités sélectionnée. Le flux original reste enregistré et prioritaire ; le secours couvre aussi Il Giornale et les principaux hôtes techniques de flux.

22. Correction d’un problème sous macOS où, lors de la fermeture d’un document modifié après avoir choisi « Ne pas enregistrer », la demande d’enregistrement pouvait apparaître une seconde fois. Sonarpad mémorise désormais la confirmation pour l’événement de fermeture en cours et ne pose la question qu’une seule fois.


23. « Sources de la communauté » affiche désormais toujours toutes les sources disponibles pour la langue sélectionnée, y compris celles déjà présentes dans la bibliothèque. Les sources déjà importées sont marquées « Déjà importée » ; leur sélection demande si elles doivent être remplacées. Le remplacement met à jour la même entrée sans créer de doublon et conserve le dossier dans lequel l’utilisateur l’avait organisée.

24. Ajout de « Aller à la date » dans les Podcasts et RaiPlay Sound, selon le fonctionnement de la version mobile. Dans les Podcasts, la commande apparaît en tête du sous-menu uniquement lorsque le flux contient de vraies dates ; choisir une date affiche la liste complète des épisodes de ce jour, y compris au-delà des 30 premiers du menu. RaiPlay Sound affiche un bouton contextuel « Aller à la date » lorsque des contenus datés sont disponibles. Les sélecteurs affichent directement uniquement les dates disponibles, sans libellés redondants, pour une navigation VoiceOver plus claire.

25. Ajout de « Découper un fichier multimédia… » dans Outils > Multimédia. Le nouveau Media Cutter reprend sur Mac le flux de la version mobile avec des modes guidé et avancé : choix du début et de la fin de coupe, écoute et réglage jusqu’à 0,10 seconde, division en plusieurs parties, suppression et restauration non destructives, aperçu, rotation vidéo et possibilité d’ajouter une nouvelle piste audio avec volumes séparés et répétition facultative. L’enregistrement utilise un fichier temporaire, annonce le démarrage à VoiceOver, affiche la progression et peut être interrompu sans modifier le fichier original ni laisser un résultat incomplet. Les contrôles non pertinents sont masqués afin d’éviter les annonces redondantes de VoiceOver.

Version 0.4.0 - 3 septembre 2026

Audiodescription avec IA — nouvelle fonction principale

- Ajout de « Créer une audiodescription avec l’IA » directement dans le menu Outils. Sonarpad analyse l’audio pour trouver les espaces sans dialogue, génère les descriptions avec Gemini et utilise les moteurs vocaux déjà disponibles, sans parler par-dessus les dialogues.

- Amélioration de la synchronisation entre ce qui se passe dans la vidéo et les descriptions, avec des contrôles automatiques des temps générés par Gemini.

- « Activer les pauses étendues » est désactivé par défaut. Cette option peut être activée pour les contenus avec beaucoup de dialogues ou peu d’espace disponible afin de permettre l’insertion de descriptions plus longues.

- Sonarpad peut essayer de reconnaître les personnages et d’utiliser leurs noms. Les catalogues de personnages peuvent être conservés entre les épisodes d’une série afin d’améliorer la continuité.

- Les projets peuvent être enregistrés, les descriptions modifiées ultérieurement puis exportées à nouveau sans tout régénérer avec Gemini.

- Si le processus est interrompu, Sonarpad conserve la progression et permet de reprendre l’audiodescription. Si le quota Gemini est épuisé, il est possible d’attendre, de changer de modèle ou d’arrêter sans perdre le travail déjà terminé.

- La fenêtre permet de choisir la langue, le niveau de détail, le modèle Gemini, le moteur vocal et la voix, et mémorise les préférences utilisées. Le module est disponible dans les langues prises en charge par Sonarpad pour Mac.

- Pendant la génération, l’interface affiche la progression, l’état actuel et Annuler ; une fois terminé, le MP3 peut être ouvert directement dans le lecteur interne.

- Amélioration de la compatibilité avec les vidéos MKV : Sonarpad gère plus fiablement les horodatages irréguliers ou absents et, lorsque cela est possible, ignore les paquets corrompus sans interrompre l’audiodescription.

- Correction d’un problème qui pouvait faire échouer l’exportation finale en MP3 avec les vidéos contenant un son multicanal, par exemple Dolby 5.1. Sonarpad convertit automatiquement le son multicanal en stéréo lorsque cela est nécessaire pour l’encodage MP3.

- Lorsqu’une vidéo contient plusieurs pistes audio, Sonarpad demande quelle piste utiliser avant le traitement. La liste déroulante accessible se parcourt avec les flèches ; OK démarre l’audiodescription avec la piste choisie, tandis qu’Annuler ferme la fenêtre et rend le focus à l’éditeur Sonarpad.

- Ajout de la case « Afficher la clé API » à côté de la clé Gemini. La clé reste masquée par défaut et n’est affichée que temporairement lorsque la case est activée ; à la réouverture de la fenêtre, elle est de nouveau masquée.

YouTube et streaming

- L’expérience YouTube a été nettement améliorée : la recherche et la navigation sont plus rapides et le bon fonctionnement a été rétabli.

- Les options de qualité vidéo sont désormais traduites : à la place de la valeur technique « best », Sonarpad affiche un libellé clair dans la langue de l’interface.

- Sonarpad mémorise le dernier format choisi dans Enregistrer le média. Par exemple, si MP4 est sélectionné, MP4 reste présélectionné à la prochaine ouverture du dialogue.


Remerciements

- Un grand merci à Leonardo Graziano et Tiziano Ferraro, qui ont testé en profondeur la fonction d’Audiodescription avec IA ainsi que Sonarpad en général, contribuant de manière précieuse à son amélioration.

- Un grand merci également au groupe Tecnologia Accessibile pour son soutien, ses tests et ses suggestions.

Version 0.3.1 - 16 juillet 2026

- Correction d’un problème qui empêchait Sonarpad de démarrer lorsque le menu Radio contenait des favoris, en raison d’identifiants de menu wxWidgets non valides.

- Sonarpad est désormais également disponible en français, espagnol, portugais, tchèque et polonais, en plus de l’italien et de l’anglais.

- Ajout d’un réglage distinct pour la Langue des actualités. Ce réglage est indépendant de la langue de l’interface et permet à Sonarpad d’utiliser des sources et des services adaptés à la langue sélectionnée.

- Ajout de la fonction Météo, qui permet de rechercher une ville et de consulter les conditions actuelles, la température, les précipitations, le vent et l’humidité, ainsi que les prévisions pour aujourd’hui, demain ou un autre jour.

- Ajout de la section Films au cinéma, avec les films actuellement à l’affiche, les prochaines sorties, les résumés, les dates de sortie et, lorsqu’ils sont disponibles, les liens vers les bandes-annonces.

- Ajout d’un calendrier accessible dans le menu Outils. Il est possible de sélectionner n’importe quelle date, de consulter les jours fériés, le saint et la citation du jour, de créer des rappels et d’ajouter des rendez-vous directement dans Calendrier de macOS.

- Ajout de la fonction Recherche d’itinéraires, qui permet de calculer des trajets à pied, à vélo, en voiture ou accessibles en fauteuil roulant. Il est possible de choisir l’itinéraire le plus rapide ou le plus court et de consulter la distance, la durée estimée et les indications détaillées.

- Ajout de la fonction Convertir les médias, qui permet de convertir des fichiers audio et vidéo dans plusieurs formats, notamment MP3, M4A, M4B, MP4, AVI, MOV, Opus, OGG, FLAC, WAV et AIFF. Il est également possible de créer une vidéo à partir d’un fichier audio et d’une image.

- Ajout du Dictionnaire vocal. Il est possible de définir des mots ou des expressions que le synthétiseur vocal doit remplacer pendant la lecture afin de corriger les prononciations, les abréviations et certains noms.

- La section Articles a été enrichie avec les commandes Articles récents et Partager, afin de retrouver rapidement les derniers contenus lus et de partager des articles avec les services disponibles sur macOS.

- Ajout dans le menu Articles des fonctions Ajouter une source d’actualités à la communauté Sonarpad et Sources d’actualités de la communauté Sonarpad. Il est possible de proposer un flux RSS ou un site d’actualités et d’importer les sources partagées par d’autres utilisateurs. Les sources sont ajoutées et affichées selon la Langue des actualités sélectionnée.

- Amélioration de la gestion des sources d’actualités. Le changement de Langue des actualités charge désormais les sources par défaut appropriées sans supprimer les sources ajoutées personnellement par l’utilisateur.

- La recherche de radios a été enrichie avec la navigation par langue, pays et ville, ainsi qu’avec les noms complets et localisés des pays.

- Ajout de la possibilité de proposer une station de radio à la communauté Sonarpad en indiquant son nom, son adresse de diffusion, sa langue et son genre.

- Ajout de l’enregistrement radio et de la programmation des enregistrements radio. Ces actions sont disponibles dans les résultats de recherche et les favoris, et les enregistrements sont sauvegardés directement au format MP3. Après avoir ouvert une station de radio, il est également possible de démarrer l’enregistrement en appuyant sur la lettre R.

- Ajout dans le menu Fichier de la liste des documents texte récemment ouverts, afin de pouvoir les rouvrir plus rapidement.

- Ajout du mode Lecture seule, utile pour consulter un document sans le modifier accidentellement.

- Ajout du Sommaire du livre pour les fichiers EPUB qui contiennent une table des matières. Il est possible de sélectionner un chapitre et d’y accéder directement.

- Ajout de la possibilité de choisir entre les voix Microsoft de haute qualité et les voix système de macOS.

- Ajout d’une option permettant d’ignorer pendant la lecture les pauses provoquées par les lignes vides.

- Ajout d’un réglage permettant de choisir le nombre de secondes à avancer ou à reculer pendant la lecture multimédia.

- Amélioration de l’accessibilité des fenêtres, des menus et des commandes, avec une gestion plus cohérente du focus, des touches Entrée et Échap et des raccourcis clavier.

- Amélioration de la localisation des messages, des boutons et des boîtes de confirmation dans toutes les langues prises en charge.

- Correction d’un problème qui empêchait l’affichage des fichiers multimédias dans la fenêtre vidéo du lecteur.

- Correction de nombreux problèmes affectant la stabilité, la lecture multimédia, les enregistrements radio programmés, la gestion des sources et la compilation sous macOS.

- Remerciements particuliers à Leonardo Graziano, Luca Maianti et au groupe italien Tecnologia Accessibile pour leur soutien continu et leurs tests bêta réguliers.

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
