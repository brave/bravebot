# French. Written against locales/en-US.ftl, which owns the set of messages and the name and
# kind of every argument.
#
# Glossary, so that one thing is called one thing throughout:
#
#   planner      le planificateur   the model holding the conversation
#   processor    le processeur      an isolated model handed slots and nothing else
#   turn         le tour            one round of work, from a prompt to a reply
#   trusted      fiable             content the planner is allowed to read
#   untrusted    non fiable         content it is not
#   to vouch     approuver          a person saying they have read something
#   workspace    l'espace de travail
#   transcript   la transcription
#   scroller     le défilement      the mode Ctrl-O opens over the transcript
#   token        le jeton
#
# What is deliberately left in English: the names of commands (`/model`, `/add-dir`), of
# environment variables (`$EDITOR`), of release channels, and the letters a question is
# answered with. Those are typed rather than read.


## Compter

count-turns = { $count ->
    [one] { $count } tour
   *[other] { $count } tours
    }


## Le démarrage, et les mots affichés avant qu'une interface existe

cli-tagline =
    bravebot { $version } : un agent polyvalent résistant à l'injection de prompt
cli-usage-heading = Utilisation :
cli-usage-interactive = Démarrer une session interactive
cli-usage-plain = Démarrer une session en lignes, sans rien prendre au terminal
cli-usage-task = Exécuter une seule tâche
cli-usage-piped = ... avec une entrée redirigée, jamais fiable
cli-usage-resume = Reprendre une session dans ce répertoire
cli-usage-continue = Reprendre la session la plus récente de ce répertoire
cli-usage-fork = Dupliquer une session pour explorer une autre voie
cli-usage-doctor = Vérifier la configuration et le confinement
cli-usage-import = Importer un abonnement Leo Premium

cli-keys-heading = Touches interactives :
cli-key-send = Envoyer
cli-key-audit = Afficher ou masquer le journal d'audit
cli-key-history = Revenir sur les invites envoyées
cli-key-history-search = Rechercher parmi les messages envoyés
cli-key-scroll = Faire défiler la transcription
cli-key-jump = Aller au début ou au plus récent
cli-key-cancel = Annuler un tour en cours, vider la saisie, ou partir
cli-key-leave = Partir

cli-commands-heading = Commandes interactives :
cli-name-a-file = Inclure un fichier de l'espace de travail comme contexte fiable

## Une session en lignes : pas d'écran à elle, pas de couleur, rien de redessiné

cli-plain-opening =
    bravebot { $version } en lignes, { $model }. Une ligne est une demande ; la fin de
    l'entrée (Ctrl-D) termine la session.
# Dit lorsque --plain est donné avec autre chose qu'un terminal sur l'entrée standard. Les lignes
# qu'il lit sont des demandes, et rien ne se porte garant de ce qu'un tube transporte.
cli-plain-needs-a-terminal =
    --plain lit ce que vous tapez, donc son entrée doit être un terminal. Utilisez -p pour
    exécuter une seule tâche avec une entrée redirigée, lue comme un contexte mis en quarantaine.
# Dit lorsque --plain est donné à côté d'une autre manière de démarrer. Il démarre une session
# plutôt qu'il ne la décrit, donc il n'y a rien à combiner avec lui.
cli-plain-takes-nothing-else =
    --plain démarre une session et ne prend aucun autre argument. --incognito,
    --dangerously-skip-permissions et --settings vont avec lui ; tout le reste est une autre
    manière de démarrer.

mode-accept-edits = ⏵ modifications acceptées
mode-plan = ⏸ mode plan
mode-bypass = ⏵⏵ permissions contournées

cli-options-heading = Options :
cli-option-file = Inclure un fichier de l'espace de travail comme contexte (répétable)
cli-option-add-dir = Accéder à un répertoire hors de celui de travail (répétable)
cli-option-settings = Lire ce fichier de réglages pour cette exécution, au-dessus de ceux trouvés sur le disque
cli-option-mode = turn (par défaut) décide étape par étape ; manifest planifie tout le déroulement d'abord
cli-option-model = Le modèle demandé par cette exécution, à la place de celui mémorisé ou configuré
cli-option-print = Non interactif. Lit l'entrée redirigée comme contexte en quarantaine
cli-option-trace = Afficher le journal d'audit
cli-option-json = Afficher un objet de résultat sur stdout au lieu de la réponse
cli-option-incognito = Ne rien écrire dans ~/.bravebot : ni historique, ni session, ni préférence
cli-option-vet =
    Pour cette exécution, laisser une vérification répondre : le contenu où elle ne trouve rien est
    promu sans vous demander, et quand personne ne peut être consulté, tout le reste est retenu
cli-option-dangerously-skip-permissions =
    Contourner toutes les vérifications de permission. Recommandé uniquement pour des bacs à sable
    sans accès à Internet
cli-option-help = Afficher ce message
cli-option-version = Afficher la version


## Ce qu'une exécution en ligne de commande dit quand elle ne peut pas démarrer

cli-unknown-option = option inconnue : { $flag }
cli-file-needs-a-path = --file demande un chemin
cli-add-dir-needs-a-path = --add-dir demande le chemin absolu d'un répertoire
cli-settings-needs-a-path = --settings demande le chemin d'un fichier de réglages
cli-settings-not-a-file = --settings ne nomme aucun fichier : { $path }
cli-mode-needs-a-name = --mode demande l'un de : { $names }
cli-model-needs-a-name = --model demande le nom d'un modèle
cli-unexpected-argument = argument inattendu : { $argument }
cli-task-required = une tâche est requise
cli-configuration-problem = erreur de configuration : { $problem }
cli-workspace-problem = erreur d'espace de travail : { $problem }
cli-interface-problem = erreur d'interface : { $problem }
cli-directory-unknown = impossible de savoir de quel répertoire il s'agit
cli-no-such-session = aucune session { $id } dans ce répertoire
cli-manifest-run = { $id } est une exécution planifiée : il n'y a rien à poursuivre, voici ce qu'elle a fait
cli-nothing-to-continue = aucune session à reprendre dans ce répertoire
cli-fork-needs-a-name = --fork nécessite un identifiant de session
cli-piped-input-unreadable = avertissement : impossible de lire l'entrée redirigée : { $problem }
cli-piped-input-too-large =
    l'entrée redirigée dépasse { $limit } Mio. Écrivez-la dans un fichier et nommez celui-ci
    à la place


## Ce qu'une exécution dit quand aucun service de modèle n'est configuré

onboarding-no-model = aucun service de modèle n'est encore configuré
onboarding-subscription-unusable = l'abonnement enregistré n'a pas pu être utilisé : { $problem }
onboarding-name-a-configured-model =
    Un service est configuré, mais le modèle en vigueur est l'un de ceux de Brave : indiquez l'un des vôtres avec la clé `model` dans ~/.bravebot/settings.json, ou avec --model pour une exécution unique. `bravebot doctor` indique ce que propose chaque service configuré.
onboarding-pick-one = Configurez l'une de ces options, puis relancez bravebot :
onboarding-bedrock =
    AWS Bedrock, via votre propre compte : ajoutez un bloc `provider` nommé `amazon-bedrock` dans ~/.bravebot/settings.json, avec sa région et les modèles à proposer.
onboarding-openrouter =
    OpenRouter, ou toute autre passerelle compatible OpenAI : ajoutez un bloc `provider` à son nom dans ~/.bravebot/settings.json, avec la variable qui contient sa clé d'API et les modèles à proposer.
onboarding-leo =
    Brave Leo Premium, si vous y êtes déjà abonné : lancez `bravebot import-leo-creds` sur une machine où Brave est connecté à cet abonnement. Les modèles passent alors par la passerelle IA de Brave, dont certains problèmes restent à résoudre, donc préférez pour l'instant l'une des deux options ci-dessus.
onboarding-where-to-read =
    Des exemples concrets se trouvent dans https://github.com/brave/bravebot/blob/main/docs/getting-started.md#choosing-a-model-service


## Ce qu'une exécution unique dit à côté de la réponse

cli-notice = note : { $notice }
cli-model-used = modèle : { $model }
cli-something-was-refused =
    note : un contrôle de la politique a refusé quelque chose pendant ce tour
cli-resume-heading = Reprenez cette session avec :
# Quand /cd a déplacé la session, le shell où ceci s'affiche n'est pas là où se trouve
# l'enregistrement, et --resume cherche un identifiant sous le répertoire où il est lancé.
cli-resume-moved = Cette session s'est déplacée vers { $directory }. Reprenez-la depuis là avec :


## L'état de la configuration et du confinement
#
# Ces noms sont posés dans une colonne de dix caractères : au-delà, la valeur qu'ils nomment
# ne s'aligne plus sur les autres.

doctor-configuration-ok = configuration OK
doctor-endpoint = adresse
doctor-premium = premium
doctor-premium-absent = non configuré
doctor-key-id = id de clé
doctor-model = modèle
doctor-model-chosen = { $model } (choisi avec /model)
doctor-model-default = { $model } (par défaut)
doctor-key-name = clé
doctor-key = { $key } (jamais transmise)
doctor-ends = fin
doctor-ends-signing-key =
    la clé de signature : émise par le service Brave, qui dérive sa copie d'une graine maîtresse et de cet id de clé ; elle ne prend fin qu'en retirant cet id là-bas et en publiant une autre version, car la clé d'une version est celle de toutes les installations
doctor-ends-aws-access-key =
    une clé d'accès permanente : émise par AWS IAM à l'utilisateur nommé par le profil ; supprimée avec `aws iam delete-access-key`
doctor-ends-aws-session =
    une identification de session : émise par AWS STS pour le profil et prend fin à sa propre expiration ; on ne peut y mettre fin plus tôt qu'auprès de son émetteur, car `aws sso logout` efface la copie de cette machine et non la session elle-même
doctor-outlives = survit
doctor-outlives-aws-access-key =
    une identification de session déjà émise par STS sous cette clé d'accès, qui court jusqu'à sa propre expiration : la suppression de la clé ne l'atteint pas
doctor-backend = service
doctor-backend-bedrock = AWS Bedrock
doctor-backend-aichat = Brave Leo
doctor-backend-gateway = { $gateway } (passerelle)
doctor-gateway-token = trouvé (jamais affiché)
doctor-gateway-token-absent = aucun trouvé (définissez une variable nommée dans `env`)
doctor-gateway-token-not-needed = aucun requis (le bloc n'en nomme aucun)
doctor-gateway-models-absent = aucun configuré (la passerelle est interrogée)
doctor-region = région
doctor-profile = profil
doctor-profile-absent = identifiants par défaut
doctor-tiers = modèles
doctor-tiers-absent = aucun configuré (définir ANTHROPIC_DEFAULT_OPUS_MODEL)
doctor-settings = réglages
doctor-settings-names = { $names }
doctor-settings-absent = aucun settings.json
doctor-permissions = permissions
doctor-permissions-absent = aucune règle
doctor-permissions-count =
    { $count ->
        [one] { $count } règle
       *[other] { $count } règles
    }
doctor-permissions-unreadable = règle illisible
doctor-settings-no-variables = settings.json, ne nommant aucune variable
doctor-settings-layer = couche
doctor-settings-override = remplacement
doctor-settings-overridden = { $name } depuis { $path }
doctor-settings-ignored = ignoré
doctor-settings-vetting-ignored =
    vetting.auto dans { $path } n'est pas appliqué : il n'est lu que depuis
    ~/.bravebot/settings.json
doctor-settings-allow-ignored =
    la règle allow { $rule } dans { $path } n'est pas accordée : une règle allow répond à une
    invite, le fichier d'un projet la propose donc et vous l'accordez au démarrage d'une session
doctor-settings-granted = accordée
doctor-settings-allow-granted =
    la règle allow { $rule } dans { $path } est accordée pour ce répertoire
doctor-managed = géré
doctor-managed-pinned = { $names } depuis { $path }
doctor-managed-nothing = { $path }, n'épinglant rien
doctor-leo = leo
doctor-subscription =
    abonnement { $environment } importé, { $unspent } identifiants sur { $total } non dépensés
doctor-state-directory = répertoire d'état { $path }, depuis { $variable }
doctor-state-directory-unprotected = non restreint
doctor-state-directory-permissions =
    l'historique des invites, les enregistrements de session et les choix retenus portent les
    permissions de votre répertoire de profil
doctor-state-directory-absent = aucun répertoire d'état : { $variables } ne nomme rien
doctor-state-directory-not-kept = non conservés
doctor-state-directory-forgotten =
    les sessions et --resume, l'historique des invites, le modèle et le thème que vous choisissez
doctor-state-directory-not-read = non lus
doctor-state-directory-your-own =
    vos propres réglages, compétences et instructions permanentes ; ceux d'une copie de travail
    s'appliquent quand même
doctor-state-directory-remedy = pour les garder
doctor-state-directory-set-profile = réglez { $variables } sur un répertoire à vous
doctor-confinement = confinement { $level }
confinement-kernel = imposé par le noyau
confinement-partial = partiel
confinement-none = aucun
doctor-mechanisms = mécanismes
doctor-network-denial = refus réseau
doctor-kernel-enforced = imposé par le noyau
doctor-not-enforced = NON imposé
doctor-confinement-unavailable = confinement indisponible
doctor-network = réseau
doctor-trust-roots = racines de confiance
doctor-trust-roots-bundled = intégrées ({ $variables } en désigne d'autres)
doctor-trust-roots-named = { $paths }
doctor-trust-roots-none = aucune, donc toute connexion échouera
doctor-trust-roots-unusable = inutilisable
doctor-proxy = proxy
doctor-proxy-absent = aucun ({ $variables } en désigne un, en majuscules ou en minuscules)
doctor-proxy-in-force = { $proxy }
doctor-proxy-authenticated = { $proxy } (avec un identifiant, jamais affiché)
doctor-proxy-unsupported = { $protocol } n'est pas pris en charge par cette version, les requêtes sont directes
doctor-no-proxy = sans proxy


## Une règle de permission que cette version n'a pas pu appliquer

# Dit partout où une règle écartée est signalée : par `doctor`, sous l'étiquette ci-dessus, et
# comme note dans la session qui a lu le fichier. L'entrée est citée telle que le fichier l'a
# écrite, parce que la retrouver est tout l'intérêt d'en être averti.
permission-rule-unreadable = '{ $rule }' { $problem }
permission-rule-not-a-line = n'est pas une règle ; une règle est une ligne de texte
permission-rule-empty = est vide
permission-rule-unclosed-bracket = n'a pas sa parenthèse fermante
permission-rule-unknown-family = ne nomme aucune famille d'outils de cet agent ; utilisez Read, Edit ou Bash
permission-rule-empty-brackets = a des parenthèses vides ; enlevez-les pour viser toute utilisation
permission-rule-unanchored = a besoin d'un répertoire personnel ou d'un répertoire de réglages pour indiquer vers quoi elle pointe
permission-rule-not-a-domain-rule = a besoin d'un domaine ; écrivez WebFetch(domain:example.com)
permission-rule-no-domain-named = ne nomme aucun domaine après 'domain:'


## Importer un abonnement Leo Premium

leo-no-premium-endpoint =
    avertissement : cette version n'a pas d'adresse premium, les identifiants importés ne
    seront donc pas utilisés
leo-set-and-rebuild = définissez { $variable } et recompilez
leo-unknown-channel = canal inconnu : { $channel }
leo-expected-channel = attendu parmi : stable, beta, nightly, development
leo-forgotten = abonnement importé oublié
leo-not-while-incognito = un import enregistre des identifiants sur le disque, ce qu'une session incognito ne fera pas
leo-looking = recherche d'un abonnement Leo dans Brave { $channel }
leo-found = abonnement { $environment } trouvé : { $order }
leo-registering = enregistrement de cette installation comme nouvel appareil
leo-stored =
    { $count } identifiants enregistrés dans { $path }, valables jusqu'au { $expiry }
leo-browser-untouched =
    les requêtes premium les utiliseront désormais ; les identifiants du navigateur n'ont
    pas été touchés

subscription-unusable =
    l'abonnement importé n'a pas pu être utilisé ({ $problem }) ; ce tour n'en utilise
    donc aucun

background-job-finished = `{ $command }` s'est terminé en arrière-plan : { $outcome }

hook-not-started = le hook { $moment } `{ $program }` n'a pas pu être démarré ({ $detail })
hook-failed = le hook { $moment } `{ $program }` s'est mal terminé ({ $status })
hook-stopped =
    le hook { $moment } `{ $program }` tournait encore après { $seconds } secondes et a été
    arrêté


## Approuver un répertoire, demandé une fois quand une session démarre ailleurs

trust-directory-title = faire confiance à ce répertoire ?
trust-directory-question = Approuver
trust-directory-explained =
    Les fichiers d'ici seront lus comme fiables, et les modifications qui leur sont
    apportées ne vous seront pas montrées une par une. Répondez non si ce code n'est pas
    le vôtre.
trust-directory-regardless =
    Dans tous les cas, tout ce qui vient du web ou d'un fichier non fiable vous est encore
    montré avant d'être écrit.
trust-directory-yes = lui faire confiance
trust-directory-no = me demander à chaque écriture
quit = quitter
trust-quit-again = encore


## Ouvrir les répertoires qu'un fichier de réglages nomme, demandés une fois chacun au démarrage

named-directory-title = ouvrir ce répertoire ?
named-directory-question = Ouvrir
named-directory-explained =
    Un fichier de réglages a demandé que ce répertoire soit ouvert à côté de celui où vous
    travaillez. L'ouvrir permet d'y lire et d'y modifier des fichiers, et de les lire comme
    fiables.
named-directory-regardless =
    Un fichier ne peut pas ouvrir un répertoire de lui-même. Répondez non et cette session tourne
    sans lui ; /add-dir en ouvre un à tout moment.
named-directory-yes = l'ouvrir
named-directory-no = le laisser fermé


## Accorder les règles allow proposées par le fichier de réglages d'un dépôt, demandé une seule fois

granted-rules-title = accorder ces règles de permission ?
granted-rules-question = Les réglages de ce projet demandent à ne plus vous interroger sur :
granted-rules-explained =
    Chacune de ces règles répond à une demande d'approbation que vous verriez autrement : lancer un
    programme, écrire un fichier, ou récupérer une URL. Elles ont été écrites par l'auteur de ce
    projet, pas par vous.
granted-rules-regardless =
    Un projet ne peut pas se les accorder lui-même. Répondez non et cette session vous interroge
    sur chaque action comme d'habitude ; les règles de ~/.bravebot/settings.json sont les vôtres et
    s'appliquent toujours.
granted-rules-yes = les accorder
granted-rules-no = continuer à me demander


## Choisir un thème, un modèle, ou une session à reprendre

theme-picker-title = thèmes
theme-picker-keys = ↑↓ choisir  ·  Entrée valider  ·  Échap garder l'actuel
model-picker-heading = Choisir un modèle
model-picker-keys =
    ↑↓ choisir  ·  Entrée valider  ·  tapez pour rechercher  ·  Échap garder l'actuel
model-picker-search-placeholder = Rechercher
model-picker-nothing-matches = aucune correspondance
picker-current = actuel

config-picker-title = mode d'édition
config-picker-keys = ↑↓ choisir  ·  Entrée valider  ·  Échap garder l'actuel
config-editing-hint-ordinary = les flèches et les raccourcis readline
config-editing-hint-vi = édition modale, avec hjkl et les opérateurs

effort-picker-title = effort
effort-picker-keys = ↑↓ choisir  ·  Entrée valider  ·  Échap garder l'actuel
effort-unset = par défaut
effort-hint-unset = laissé au service qui répond
effort-hint-low = réflexion minimale, pour le travail simple
effort-hint-medium = moins de réflexion, quand cela suffit
effort-hint-high = la quantité habituelle, pour le travail soigné
effort-hint-xhigh = plus de réflexion, pour le code et les longs runs
effort-hint-max = le maximum de réflexion, coût mis à part
picker-premium = premium
picker-service-brave = Brave
picker-service-bedrock-profile = Bedrock, votre profil AWS { $profile }
picker-service-bedrock = Bedrock, votre compte AWS
history-search-title = Rechercher un message
history-scope-everywhere = partout
history-scope-here = ce projet
history-search-placeholder = Filtrer l'historique…
history-search-keys =
    ↑↓ pour se déplacer  ·  Entrée pour utiliser  ·  { $scope } pour la portée  ·  Échap pour annuler
history-search-nothing-matches = aucune correspondance
history-search-more-lines =
    { $count ->
        [one] … +1 ligne
       *[other] … +{ $count } lignes
    }
history-age-now = à l'instant
history-age-minutes = il y a { $count } min
history-age-hours = il y a { $count } h
history-age-days = il y a { $count } j
history-age-months = il y a { $count } mois
input-history-position = Historique { $index }/{ $total }
input-history-search = { $chord } pour rechercher
input-history-scope = { $chord } ce projet
resume-heading = Reprendre une session
resume-search-placeholder = Rechercher…
resume-keys =
    ↑↓ pour choisir  ·  Entrée pour reprendre  ·  tapez pour rechercher  ·  Échap pour une
    nouvelle session
resume-nothing-matches = aucune correspondance
resume-manifest-run =
    c'était une exécution manifest, qui ne peut pas être reprise ; démarrez une nouvelle
    session


## Commun à toutes les questions que l'interface s'arrête pour poser

stop-the-turn = arrêter le tour
scroll-more = ↑↓ { $count } de plus
scroll-back = ↑↓ retour


## Approuver une écriture

write-title = approuver cette écriture ?
write-create = Créer
write-overwrite = Remplacer
write-edit = Modifier
write-tally = +{ $added } -{ $removed }
write-too-large-to-show =
    le changement est trop grand pour être montré : { $added } lignes en remplacent
    { $removed }
write-untrusted = non fiable : personne n'a lu ceci, et le modèle ne l'a jamais vu
write-remark =
    ce que le processeur isolé a dit de ce changement, que rien n'a vérifié par rapport à lui
write-credentials =
    ceci semble déposer un secret dans l'arbre, d'après le nom à côté de la valeur et l'allure de
    la valeur. Rien ne l'a reconnu comme la clé d'un fournisseur précis : c'est donc une
    supposition, et c'est à vous d'en décider
write-unchanged = { $count ->
    [one] … { $count } ligne inchangée
   *[other] … { $count } lignes inchangées
    }
write-yes = l'écrire
write-no = ne rien changer


## Approuver une commande

run-title = exécuter ceci ?
run-verb = Exécuter
run-stages = { $count ->
    [one] { $count } étape
   *[other] { $count } étapes
    }
run-in-directory = dans { $directory }
watching-list-command = commande
watching-list-aside = aparté
watching-aside-head = une question posée à côté du travail
watching-aside-question = vous avez demandé
watching-aside-answer = la réponse, que la conversation n'a pas lue
watching-aside-not-kept = cette réponse n'est que sur votre écran : la conversation avait lu quelque chose de non fiable, donc l'enregistrement ne la garde pas
watching-aside-gone = l'enregistrement n'a pas pu garder cette réponse, elle n'est donc pas revenue avec la session
watching-lines = { $count ->
    [one] 1 ligne
   *[other] { $count } lignes
    }
watching-output-head = ce que cette commande a affiché
watching-output-read = le modèle a lu ceci
watching-output-kept = le modèle n'a pas lu ceci
watching-row-read = lu
watching-row-kept = non lu
watching-row-kept-answer = gardée
watching-row-screen-only = écran seulement
watching-output-more = { $count ->
    [one] 1 ligne de plus a été affichée et n'est pas conservée
   *[other] { $count } lignes de plus ont été affichées et ne sont pas conservées
    }
run-line-sent = le modèle a écrit :
run-writes = il écrit ces fichiers :
run-is-fed = le contenu de ceci lui est fourni :
run-not-sandboxed =
    ceci n'est pas isolé : l'exécution a les mêmes accès que votre propre shell
run-releases-private =
    vos propres données lui sont aussi fournies, et elles partent d'ici avec elle
run-always-explained = a : approuver cette commande exacte pour le reste de cette session
run-always-means-both = ce qui veut dire les deux :
run-always-runs-again = elle s'exécute de nouveau sans rien demander, effets de bord compris
run-always-output-trusted = ce qu'elle affiche est fiable, et le modèle le lit
run-always-exact-arguments = ces arguments seulement : git log ne couvrirait pas git push
run-always-this-directory = ce répertoire seulement : la même ligne ailleurs est redemandée
run-private-not-remembered =
    une entrée privée est soumise à chaque fois, celle-ci ne peut donc pas être retenue
run-assignment-not-remembered =
    une affectation placée devant un programme est soumise à chaque fois, celle-ci ne peut donc pas être retenue
run-write-not-remembered =
    une ligne nommant un fichier à écrire est soumise à chaque fois, celle-ci ne peut donc pas être retenue
run-remember-explained =
    r : ne plus rien demander pour cette ligne exacte, dans ce répertoire, à partir de maintenant
run-remember-where = elle est écrite ici, et supprimer la ligne est le chemin du retour :
run-remember-only-asking =
    cela arrête seulement la question : ce qu'elle affiche reste en quarantaine
run-remember-every-session =
    toute session ouverte dans ce répertoire la lit, pas seulement celle-ci
run-pattern-varies =
    ces arguments diffèrent de ceux qui vous ont déjà été soumis : aucune touche ici n'arrête la question
run-pattern-where =
    un motif pour la famille s'écrit dans un fichier de configuration, il ne se répond pas ici :
run-pattern-covers-unread =
    un motif couvre des lignes que personne n'a lues, ce qui est plus que ce qu'accorde toute touche ici
run-pattern-only-asking =
    un motif arrête la question et rien d'autre : ce que la ligne affiche reste en quarantaine
run-yes = l'exécuter
run-always = toujours pour cette session
run-remember = s'en souvenir
run-no = ne pas l'exécuter


## Ce qu'une vérification a dit, en tête de chaque question dont la réponse sortirait un contenu de quarantaine

# Dit d'une sortie de commande, d'un fichier qu'on vous propose d'approuver et d'un emplacement dont
# le modèle a demandé la lecture, donc « ceci » plutôt qu'un nom : la question autour a déjà dit de
# quoi il s'agit.
check-safe = la vérification n'a trouvé aucune tentative de donner des instructions ici
check-unsafe = la vérification estime que ceci ressemble à une tentative de donner des instructions
check-inconclusive = la vérification n'a pas abouti, donc rien n'a examiné ceci


## Laisser le modèle lire ce qu'une commande a affiché

output-title = laisser le modèle lire ceci ?
output-verb = Lire
output-lines = { $count ->
    [one] { $count } ligne
   *[other] { $count } lignes
    }
output-printed-by = affiché par { $command }
output-unseen =
    le modèle n'a pas vu ceci. L'approuver le met dans son contexte, et il agira dessus.
output-empty = (rien n'a été affiché)
output-yes = le laisser lire ceci
output-no = le garder pour vous


## Laisser le modèle lire un emplacement mis en quarantaine qu'une vérification a examiné

vet-title = laisser le modèle lire ceci ?
vet-verb = Lire
vet-lines = { $count ->
    [one] { $count } ligne
   *[other] { $count } lignes
    }
vet-from = provenance : { $origin }
vet-unseen =
    le modèle n'a pas vu ceci. L'approuver le met dans son contexte, et il agira dessus.
vet-covers-this-only =
    ceci ne couvre que ce qui suit. Aucun chemin n'est approuvé, donc la prochaine lecture de
    la même chose posera de nouveau la question.
vet-expected = le modèle a demandé ceci en attendant { $expects }
vet-empty = (il n'y a rien dedans)
vet-yes = le laisser lire ceci
vet-always = ne plus demander
vet-no = le garder pour vous
vet-always-covers =
    a supprime cette question partout où une vérification ne trouve rien, dans cette session et
    la suivante, jusqu'à ce que vous changiez d'avis. Conservé dans ~/.bravebot/vetting.


## Récupérer une URL

fetch-title = récupérer ceci ?
fetch-verb = Récupérer
fetch-host = communication avec { $host }
fetch-explained =
    ce qui revient reste en quarantaine quelle que soit votre réponse : le modèle peut le
    confier à un processeur ou l'écrire dans un fichier, et ne peut ni le lire ni savoir
    ce qu'il contient.
fetch-yes = le récupérer
fetch-no = ne pas le récupérer


## Démarrer un serveur de langage

server-title = démarrer un serveur de langage ?
server-verb = Démarrer
server-workspace = pour indexer { $workspace }
server-build-tooling =
    ceci lance les outils de compilation de son écosystème : le code de vos dépendances s'exécute
    donc avec vos propres accès, comme le fait cargo test. il reste actif pendant cette session.
server-reads-only =
    il lit le projet et reste actif pendant cette session. rien n'est écrit dans votre projet.
server-explained =
    ce qu'il rapporte garde le même statut quelle que soit votre réponse : un emplacement dans un
    fichier est montré, et le texte à cet emplacement est en quarantaine tant que vous n'avez pas
    approuvé le fichier.
server-yes = le démarrer
server-no = ne pas le démarrer


## Approuver un plan avant son exécution

plan-title = exécuter ce plan ?
plan-verb = Exécuter
plan-steps = { $count ->
    [one] { $count } étape
   *[other] { $count } étapes
    }
plan-goal = pour { $task }
plan-explained =
    le programme entier, décidé avant toute lecture. rien de ce qu'il lit ne peut ajouter
    une étape, en retirer une, ni envoyer quoi que ce soit ailleurs que là où ce plan le dit
    déjà.
plan-not-its-writes =
    approuver le plan n'approuve pas ses écritures. chacune vous sera encore soumise le
    moment venu.
plan-nothing-yet =
    rien n'a encore été lu ni écrit, donc refuser laisse tout en l'état.
plan-yes = l'exécuter
plan-no = ne pas l'exécuter
# Là où une question est une ligne sur un terminal plutôt qu'un panneau : à quoi ressemble un oui,
# et la seule réponse qui approuve. Toute autre ligne, et la fin de l'entrée, refuse. Partagé par
# toutes les questions posées en lignes, pour qu'un seul oui les couvre.
line-answer = [o/N]
line-answer-yes = o
# La ligne propre au plan, qui nomme ce qu'un oui exécute.
plan-answer = l'exécuter ? [o/N]


## Approuver un fichier en quarantaine

vouch-title = laisser le modèle lire ce fichier ?
vouch-verb = Approuver
vouch-explained =
    le modèle ne peut pas lire ce fichier, il travaille donc à l'aveugle dessus.
    L'approuver lui permet de le lire pour le reste de cette session, ici et à chaque
    lecture ultérieure.
vouch-nothing = (rien de ce fichier ne peut être affiché)
vouch-yes = l'approuver
vouch-no = le laisser en quarantaine


## Compter ce qu'une session accumule

count-rules = { $count ->
    [one] { $count } règle
   *[other] { $count } règles
    }
count-commands = { $count ->
    [one] { $count } commande
   *[other] { $count } commandes
    }
count-tokens = { $count ->
    [one] { $count } jeton
   *[other] { $count } jetons
    }
count-tokens-thousands = { $thousands } k jetons
# Le français écrit une virgule entre un nombre entier et sa fraction.
number-decimal-separator = ,


## Ce que /status rapporte de la session

status-session = Session
status-session-untitled = sans titre, rien n'a encore été envoyé
status-session-id = Id de session
status-directory = Répertoire
status-directory-trusted = fiable
status-directory-untrusted = non fiable, chaque écriture vous est donc montrée
status-also-open = Aussi ouvert
status-added-directory = ajouté avec /add-dir
status-scratch = Temporaire
status-scratch-note = propre à cette session, qui peut y écrire, supprimé à sa fin
status-model = Modèle
status-model-chosen = choisi avec /model
status-model-default = la valeur par défaut configurée
status-effort = Effort
status-effort-chosen = choisi avec /effort
status-effort-default = ce que le service fait de lui-même
status-effort-not-read = choisi avec /effort, mais ce modèle n'en lit aucun
status-theme = Thème
status-theme-chosen = choisi avec /theme
status-served = Répondu par
status-served-instead = servi à la place du modèle demandé
status-endpoint = Adresse
status-premium-available = premium disponible, rien encore envoyé
status-premium-in-use = premium, un jeton a été dépensé
status-premium-not-spent = aucun abonnement utilisé
status-no-subscription = aucun abonnement configuré
status-confinement = Confinement
status-loop = Boucle
status-loop-every = toutes les { $every }
status-loop-self-paced = cadencée par chaque tour
status-loop-next = prochaine dans { $next }
status-loop-running = en cours
status-loop-unpaced = en attente que le tour dise quand
status-goal = Objectif
status-watch = Veille { $number }
status-watch-armed-by = posée au tour { $turn } · il reste { $left }
# « fois » est invariable, donc une seule forme là où l'anglais en a deux.
status-goal-rounds = renvoyé { $rounds } fois, il en reste { $left }
status-permissions = Permissions
status-permissions-cycle = shift-tab pour changer
status-vetting = Vérification
status-vetting-auto =
    une vérification qui ne trouve rien donne le contenu au modèle sans demander
status-vetting-where = conservé dans ~/.bravebot/vetting
status-this-session = Cette session
status-time = Temps
status-time-inference = sur le modèle
status-time-tools = exécution des outils
status-time-stalled = en attente de vous
status-time-overhead = non attribué
status-cache = Cache du prompt, dernier tour
status-cache-read = servi depuis le cache
status-cache-written = écrit dans le cache pour le tour suivant
status-trust = Confiance
status-nothing-vouched-for = rien d'approuvé
status-trusted = fiable
status-untrusted = non fiable
status-programs = Programmes
status-every-run-is-asked = chaque exécution vous est soumise
status-nothing-vouched-this-session =
    rien n'a été approuvé pour cette session ; les lignes ci-dessous s'exécutent sans rien demander
status-trusted-commands = Commandes fiables
status-trusted-commands-note = exécutées sans rien demander, et leur sortie est fiable
status-command-in = dans { $directory }
status-remembered = Lignes mémorisées
status-remembered-note =
    exécutées sans rien demander dans ce répertoire, et leur sortie reste en quarantaine
status-remembered-this-session = mémorisée dans cette session
status-remembered-earlier = mémorisée dans une session antérieure
status-remembered-where = supprimez une ligne de { $path } pour qu'elle soit redemandée
status-remembered-and-more = { $count ->
    [one] … et 1 de plus, dont { $earlier } d'une session antérieure
   *[other] … et { $count } de plus, dont { $earlier } d'une session antérieure
    }

# Le français emprunte les trois premières abréviations telles quelles.
environment-local = local
environment-dev = dev
environment-prod = prod
environment-custom = personnalisé


## Ce que /cost rapporte de chaque tour

# Le français sépare le nombre du signe pour cent.
cost-share = { $percent } %
cost-turn = Tour { $number }
cost-before-the-first-turn = Avant le tour 1
cost-nothing-spent = rien de dépensé pour l'instant
cost-unattributed = non imputé à un tour


## L'indicateur dessiné pendant qu'un tour tourne

elapsed-seconds = { $seconds } s
elapsed-minutes = { $minutes } min { $seconds } s
indicator-tokens-read = ↓ { $tokens } jetons
indicator-tokens-written = ↑ { $tokens }
indicator-checking = { $lines ->
    [one] Vérification de { $lines } ligne
   *[other] Vérification de { $lines } lignes
    }
tokens-thousands = { $thousands } k
tokens-millions = { $millions } M
turn-done = tour { $turn } terminé
turn-failed = tour { $turn } en échec
turn-cancelled = tour { $turn } annulé


## Reprendre une session qui tournait ailleurs, ou sur autre chose

session-reopen-failed = impossible de rouvrir { $directory } : { $problem }
session-branch-moved =
    cette session tournait sur { $was } ; cette copie de travail est sur { $now }
session-branch-gone =
    cette session tournait sur { $was } ; cette copie de travail n'est sur aucune branche
session-branch-new =
    cette session ne tournait sur aucune branche ; cette copie de travail est sur { $now }
session-build-differs = cette session tournait sous bravebot { $was } ; celle-ci est { $now }


## Thèmes

theme-follows-terminal = suit votre terminal, clair ou sombre


## Répondre à une question de l'agent

ask-title = l'agent pose une question
ask-title-numbered = l'agent pose une question ({ $at } sur { $total })
ask-own-words = Répondre avec mes propres mots
ask-more-options = … { $count } de plus, utilisez les flèches
ask-key-move = déplacer
ask-key-pick-any = cocher
ask-key-pick-one = choisir
ask-key-answer = répondre
ask-key-skip = passer
ask-key-skip-question = passer la question
ask-key-back-to-options = revenir aux options


## Confier la ligne à un éditeur

editor-none-configured =
    aucun éditeur trouvé : réglez $VISUAL ou $EDITOR sur celui que vous voulez
editor-scratch-unusable = le fichier à éditer n'a pas pu être utilisé : { $problem }
editor-named-but-missing =
    '{ $command }' est introuvable, et $VISUAL ou $EDITOR le nomme, rien d'autre n'a donc
    été essayé
editor-exited-badly =
    { $editor } s'est terminé avec le code { $code }, la ligne est donc inchangée
editor-was-stopped = { $editor } a été arrêté avant de finir, la ligne est donc inchangée
editor-would-not-start = { $editor } n'a pas démarré : { $problem }


## La transcription

input-placeholder = Demandez n'importe quoi à Brave Bot
quarantined-heading = non fiable · { $origin } · { $label }
transcript-more-lines = { $count ->
    [one] … { $count } ligne de plus
   *[other] … { $count } lignes de plus
    }
transcript-unchanged = { $count ->
    [one] … { $count } ligne inchangée
   *[other] … { $count } lignes inchangées
    }
transcript-waited = { $elapsed } auprès du modèle


## Relire la transcription

scroller-title = défilement
scroller-key-line = ligne haut/bas
scroller-key-half-page = demi-page
scroller-key-full-page = page entière   (aussi ctrl-f / ctrl-b)
scroller-key-ends = début / fin   (aussi home / end)
scroller-key-prompts = invite précédente / suivante
scroller-key-search = rechercher, correspondance suivante/précédente
scroller-key-editor = ouvrir la transcription dans $EDITOR
scroller-key-this-list = cette liste
scroller-key-close = fermer le défilement   (aussi ctrl-c)
scroller-key-close-list = fermer cette liste
scroller-searching = Entrée pour rechercher  ·  Échap pour abandonner
scroller-no-matches = aucune correspondance
scroller-match-of = { $at } sur { $total }
scroller-search-keys = n suivant  ·  N précédent  ·  Échap efface  ·  q ferme
scroller-rows-below = { $count ->
    [one] { $count } ligne en dessous
   *[other] { $count } lignes en dessous
    }
scroller-footer = défilement
scroller-footer-keys = q ferme  ·  ? touches
scroller-footer-search = / rechercher


## Ce qu'une ligne commençant par une barre oblique peut être

command-status = Décrire cette session, ce qu'elle peut toucher, et ce qu'elle a dépensé
command-cost = Montrer ce que chaque tour de cette session a dépensé
command-model = Choisir avec quel modèle réfléchir
command-theme = Choisir quel thème habille l'interface
command-effort = Choisir l'effort de réflexion avant de répondre
command-config = Choisir le mode d'édition de la zone de saisie
command-add-dir = Ouvrir un autre répertoire, et l'approuver pour cette session
command-cd = Travailler désormais dans un autre répertoire, et l'approuver pour cette session
command-rename = Appeler cette conversation autrement
command-compact = Résumer la conversation jusqu'ici, en gardant la partie récente
command-btw = Demander quelque chose à côté du travail, sans le mettre dans la conversation
command-clear = Démarrer une nouvelle session ici, celle-ci restant reprenable
command-loop = Renvoyer une consigne encore et encore, dire ce qui se répète, ou l'arrêter
command-goal = Continuer à travailler jusqu'à ce qu'une condition que vous fixez soit jugée remplie
command-watch = Lister les fichiers que cette session surveille, et en arrêter un par son numéro
command-manifest = Planifier une tâche en entier, vous montrer le plan, puis l'exécuter sans rien replanifier
command-export = Exporter la transcription de la session vers un fichier markdown
command-undo = Rembobiner d'un tour et restaurer les fichiers qu'il a écrits
command-rewind = Lister les tours qu'un rembobinage peut atteindre, ou reculer d'autant
command-exit = Partir


## Ce que la session répond

session-resumed = session reprise : { $title }
session-renamed = renommée en { $title }
session-rename-needs-a-name = /rename demande un nom, comme /rename le bug de l'analyseur
session-rename-needs-something = /rename demande un nom qui contienne quelque chose
session-cleared = effacée : une nouvelle session, la précédente restant reprenable
session-rewound = session rembobinée avant le tour { $turn }
session-rewound-partly =
    session rembobinée avant le tour { $turn }, mais ces fichiers gardent ce qui a été
    écrit : { $paths }
session-nothing-to-undo = rien à annuler dans cette session
session-rewind-points = un rembobinage revient à l'un de ceux-ci, restaurant chaque ligne jusqu'à lui :
session-rewind-point =
    { $turns } en arrière : avant le tour { $turn }, { $asked }, restaure { $paths }
session-rewind-point-wrote-nothing =
    { $turns } en arrière : avant le tour { $turn }, { $asked }, aucun fichier à restaurer
session-rewind-needs-a-number = /rewind demande un nombre de tours, comme /rewind 2
session-rewind-goes-no-further =
    { $kept ->
        [one] cette session peut reculer d'un tour, pas plus
       *[other] cette session peut reculer de { $kept } tours, pas plus
    }
session-exported = transcription exportée vers { $path }
session-export-failed = impossible d'exporter la transcription : { $problem }
session-add-dir-needs-a-path = /add-dir demande un répertoire, comme /add-dir ~/notes
session-directory-added = { $directory } ajouté, et approuvé pour cette session
session-cd-needs-a-path = /cd demande un répertoire, comme /cd ~/projets/autre
session-directory-changed = travail désormais dans { $directory }, et approuvé pour cette session
# Dit une fois par répertoire qui était ouvert et ne l'est plus, pour que personne ne l'apprenne
# en se voyant refuser un fichier lisible une minute plus tôt.
session-directory-closed = { $directory } fermé ; rouvrez-le avec /add-dir { $directory }
session-directory-not-changed = impossible de passer à { $directory } : { $problem }
session-permission-rule-ignored = règle de permission ignorée dans settings.json : { $problem }
session-permission-allow-ignored =
    la règle allow { $rule } de { $path } n'est pas accordée : une règle allow répond à une invite,
    le fichier d'un projet la propose donc et c'est vous qui l'accordez
session-permission-allow-granted-before =
    la règle allow { $rule } de { $path } est accordée : vous l'avez accordée à ce projet
    auparavant ; cette réponse est conservée dans { $record }
session-permissions-skipped =
    --dangerously-skip-permissions : rien ne sera demandé avant une écriture, une commande, ou la
    lecture d'un fichier que personne n'a approuvé. shift-tab pour changer
session-directory-not-added = impossible d'ajouter { $directory } : { $problem }
session-scratch-unavailable = aucun répertoire temporaire pour cette session : { $problem }
session-using-model = utilise { $model }
session-using-model-from = utilise { $model } via { $service }
session-signing-in =
    connexion à AWS ; suivez les instructions ci-dessous, cela reprend une fois terminé
session-context-budget = compactage au-delà de { $budget } jetons, selon ce que ce modèle annonce
session-models-unavailable = impossible de lister les modèles : { $problem }
session-theme-set = thème { $theme }
session-no-such-theme = aucun thème nommé { $theme } ; essayez /theme pour la liste
session-editing-vi = édition à la manière de vi ; échap pour les commandes, i pour écrire
session-editing-ordinary = édition avec les flèches et les raccourcis readline
session-effort-set = réflexion à { $effort }
session-effort-unset = réflexion laissée au service
session-no-such-effort = aucun niveau d'effort nommé { $effort } ; essayez /effort pour la liste
session-effort-not-read = ce modèle ne lit aucun niveau d'effort ; les requêtes n'en portent pas
session-trusting = { $directory } approuvé
session-trusting-as-left = { $directory } approuvé (comme cette session l'avait laissé)
session-trusting-unasked =
    { $directory } approuvé (--dangerously-skip-permissions, la question ne vous a pas été posée)
session-not-trusting =
    ce répertoire n'est pas approuvé ; chaque écriture vous sera montrée
session-vouched-for = { $path } approuvé pour cette session
session-vetting-on =
    une vérification qui ne trouve rien donnera désormais le contenu au modèle sans vous
    demander (~/.bravebot/vetting)
session-vetting-in-force =
    une vérification qui ne trouve rien donne le contenu au modèle sans vous demander
update-available =
    bravebot { $version } est disponible (celle-ci est { $running }) ; pour la mettre à jour :
    { $command }
session-started-server = serveur de langage { $language } actif pour cette session ({ $program })
session-answered-already = déjà répondu : { $question }
session-something-was-refused =
    un contrôle de la politique a refusé quelque chose pendant ce tour
session-model-substituted =
    { $asked } n'a pas été servi : l'adresse a répondu avec { $served }. Lancez
    `bravebot doctor` si un abonnement était attendu.
session-error = erreur : { $problem }
session-no-output = aucune sortie

## Pourquoi un tour a échoué

failure-unauthorized = le service a refusé les identifiants
failure-rate-limited = le service a demandé moins de requêtes
failure-unavailable = le service n'a pas pu répondre
failure-refused = le service a rejeté la requête
failure-transport = la requête n'est pas passée
failure-incomplete = la réponse s'est arrêtée avant la fin
failure-undecodable = la réponse n'a pas pu être lue
failure-too-long = le modèle a atteint sa limite de sortie
failure-too-long-at = le modèle a atteint sa limite de sortie de { $tokens } jetons, que BRAVEBOT_OUTPUT_BUDGET relève
failure-unconfigured = rien ici n'était configuré pour envoyer la requête
failure-blocked = un contrôle local a refusé de laisser sortir la requête
failure-workspace = l'espace de travail n'a pas pu être utilisé
failure-internal = un problème est survenu ici
failure-with-status = { $what } (HTTP { $status })
failure-with-attempts = { $what }, après { $attempts } tentatives


## Répéter une consigne

loop-needs-a-prompt =
    /loop demande quelque chose à répéter, comme /loop 5m vérifie le déploiement, ou
    /loop surveille la compilation pour laisser chaque tour dire quand recommencer
loop-started-every =
    répétition toutes les { $every } ; /loop stop l'arrête, comme ctrl-c ou partir
loop-started-self-paced =
    répétition au rythme que fixe chaque tour ; /loop stop l'arrête, comme ctrl-c ou partir
# La réponse à la commande nue. La consigne en fait partie parce que la note ci-dessus a défilé,
# et qui demande ce qui se répète a le plus souvent perdu de vue ce qu'il avait lancé.
loop-active = répétition : { $prompt } · { $pace } · { $when }
loop-ends-with = /loop stop l'arrête, comme ctrl-c ou partir
loop-none =
    rien ne se répète. /loop 5m vérifie le déploiement envoie une ligne toutes les cinq minutes,
    /loop surveille la compilation laisse chaque tour dire quand recommencer, et /loop stop arrête
    l'une comme l'autre
# La partie de la ligne sous la zone de saisie qui dit qu'une boucle tourne, la seule chose à
# l'écran qui le dise entre deux passages. Courte exprès : elle partage cette ligne avec le mode et
# les mesures, et une partie qui ne tient pas dans le terminal est une partie que la ligne laisse.
loop-hint = en boucle
loop-hint-next = en boucle, prochaine dans { $next }
loop-interval-raised = l'intervalle a été relevé à { $every }, le plus rapide qu'une boucle aille
loop-interval-capped = l'intervalle a été plafonné à { $every }, le plus long qu'une boucle vive
loop-replaced = la boucle qui tournait a été remplacée
loop-tick = boucle { $count }
loop-tick-quiet = { $quiet ->
    [one] boucle { $count }, après { $quiet } passage sans rien trouver
   *[other] boucle { $count }, après { $quiet } passages sans rien trouver
    }
loop-stopped = la boucle est arrêtée
loop-aged-out = la boucle a tourné une semaine et s'est arrêtée d'elle-même
loop-unpaced = ce tour n'a pas dit quand recommencer, la boucle est donc arrêtée
loop-busy = /loop commence par un tour à lui, il attend donc la fin de celui-ci
loop-replaces-goal =
    l'objectif qui était fixé a été retiré : une session ne travaille qu'à une chose à la fois
loop-armed-by-the-turn =
    nouveau regard dans { $after }, en répétant ce que vous avez demandé ; /loop stop l'arrête,
    comme ctrl-c ou partir
loop-not-armed-under-a-goal =
    un regard plus tard a été demandé sans être lancé : cette session travaille vers un objectif,
    et elle fait une chose à la fois
loop-not-armed-under-a-watch =
    un regard plus tard a été demandé sans être lancé : cette session surveille déjà un fichier,
    et elle fait une chose à la fois


## Travailler jusqu'à ce qu'une condition soit remplie

goal-set =
    objectif : { $condition }. Rien ne démarre tant que vous n'avez pas envoyé quelque chose ;
    ensuite chaque tour est jugé par rapport à lui. Ctrl-c le retire, et partir aussi
goal-replaced = l'objectif qui était fixé a été remplacé
goal-cleared = l'objectif est retiré
goal-none =
    aucun objectif n'est fixé. /goal <condition> en fixe un, comme /goal cargo test se termine
    avec le code 0, et /goal clear le retire
goal-active = objectif : { $condition }
goal-last-check = la dernière vérification a dit : { $reason }
goal-never-checked = rien n'a encore été jugé par rapport à lui
goal-not-met = l'objectif n'est pas encore atteint : { $reason }
goal-not-met-unsaid =
    l'objectif n'est pas encore atteint, et la vérification n'a pas dit ce qui manque
goal-met = l'objectif est atteint : { $reason }
goal-met-unsaid = l'objectif est atteint
goal-impossible = l'objectif ne peut pas être atteint, il est donc retiré : { $reason }
goal-unreadable =
    la vérification n'a pas répondu par un verdict : il n'y a donc rien sur quoi agir et
    l'objectif est retiré
goal-quarantined =
    cette conversation a rencontré du contenu non fiable ; un verdict à son sujet n'est donc pas
    quelque chose sur quoi ce programme a le droit d'agir, et l'objectif est retiré
goal-spent =
    l'objectif a renvoyé le travail { $rounds } fois sans être atteint, et s'est arrêté plutôt que
    de continuer
goal-failed = l'objectif n'a pas pu être vérifié ({ $problem }), il est donc retiré
goal-uninterruptible =
    la vérification déjà en cours tient en une requête et ne peut pas être arrêtée en chemin, mais
    rien de plus ne sera envoyé
goal-ended-unexpectedly = la vérification de l'objectif s'est terminée de façon inattendue
goal-replaces-loop =
    la boucle qui tournait a été arrêtée : une session ne travaille qu'à une chose à la fois


## Être averti quand un fichier change

watch-armed =
    la veille { $number } porte sur { $path } : vous serez averti dès qu'il semblera avoir été
    écrit, sans qu'un tour tourne. /watch les liste, /watch stop { $number } arrête celle-ci, et
    ctrl-c les arrête toutes
watch-not-armed-under-a-loop =
    une veille sur un fichier a été demandée sans être posée : une boucle tourne, et une session
    ne fait qu'une seule chose à la fois qui se produise sans que personne ne tape
watch-not-armed-under-a-goal =
    une veille sur un fichier a été demandée sans être posée : cette session travaille vers un
    objectif, et elle fait une chose à la fois
watch-not-armed-full =
    une veille sur un fichier a été demandée sans être posée : { $count } sont déjà actives, le
    maximum qu'une session garde. /watch stop <n> en arrête une
watch-not-armed-unreadable =
    une veille sur { $path } a été demandée sans être posée : ce chemin ne peut pas être regardé,
    il n'y a donc rien à quoi comparer un regard ultérieur
watch-fired = veille { $number } : { $path } semble avoir été écrit
watch-listed =
    veille { $number } : { $path }, posée au tour { $turn }, il reste { $left }
watch-none =
    rien n'est sous veille. Un tour en pose une quand vous demandez à être averti au sujet d'un
    fichier, et /watch stop <n> en arrête une
watch-no-such = il n'y a pas de veille { $number }. /watch liste celles qui sont actives
watch-command-takes =
    /watch liste ce que cette session surveille, et /watch stop <n> arrête celle qui porte ce
    numéro
watch-stopped = la veille { $number } est arrêtée
watch-stopped-with-its-turn =
    la veille { $number } est arrêtée : arrêter le tour qu'elle a lancé est la façon de dire que
    vous en avez fini avec elle
watch-aged-out = la veille { $number } dure depuis une semaine et s'est arrêtée d'elle-même
watch-out-of-reach =
    la veille { $number } est arrêtée : cette session n'atteint plus le chemin qu'elle surveillait
watches-stopped = { $count ->
    [one] { $count } veille est arrêtée
   *[other] { $count } veilles sont arrêtées
    }
watches-replaced = { $count ->
    [one] { $count } veille active a pris fin : une session n'en fait qu'une à la fois
   *[other] { $count } veilles actives ont pris fin : une session n'en fait qu'une à la fois
    }


## Coller, déposer et joindre

paste-arrived-empty =
    ce collage est arrivé vide : le terminal ne transmet que du texte, une image demande
    donc { $chord }
paste-not-a-command = une image n'est pas une commande : quittez le mode shell pour en coller une
paste-not-with-a-command =
    une image ne part pas avec cette commande : envoyez-la dans une invite pour qu'elle soit vue
paste-with-the-first-tick =
    cette image part avec le premier passage de cette boucle ; les suivants disent qu'elle a été
    collée
paste-too-large = cette image fait { $size }, et un collage en porte au plus { $limit }
paste-nothing-on-clipboard = il n'y a rien à coller dans le presse-papiers
return-not-pressed =
    un autre programme a écrit cela dans le terminal : appuyez vous-même sur Entrée pour l'envoyer, ou Échap pour l'effacer
paste-folded = { $lines ->
    [one] [Texte collé #{ $number } +{ $lines } ligne]
   *[other] [Texte collé #{ $number } +{ $lines } lignes]
    }
megabytes = { $size } Mo


## Exécuter une commande que la personne a tapée

command-thread-stopped = le fil de la commande s'est arrêté de façon inattendue
command-reported-a-failure = la commande a signalé un échec


## Raccourcir une longue conversation

compact-uninterruptible = un résumé ne peut pas être interrompu ; il tient en une requête
compact-ended-unexpectedly = le résumé s'est terminé de façon inattendue
compact-done =
    { $summarised } messages antérieurs résumés, les { $kept } derniers gardés tels quels
compact-nothing-to-do = il n'y a encore rien à résumer
compact-failed = la conversation n'a pas pu être résumée : { $problem }
turn-ended-unexpectedly = le tour s'est terminé de façon inattendue
btw-needs-a-question = /btw prend la question à poser, que la conversation ne lira pas
btw-uninterruptible = la question ne peut pas être interrompue ; elle prend une requête
btw-ended-unexpectedly = la question s'est terminée de façon inattendue
btw-failed = la question n'a pas pu recevoir de réponse : { $problem }

# Ce que la session dit d'une exécution planifiée lancée depuis elle. Le plan, chaque étape et la
# réponse s'affichent au fur et à mesure ; il ne reste donc à dire qu'une exécution commence, où
# elle a été enregistrée, et ce qui a échoué là où quelque chose a échoué. Qu'une exécution ne soit
# pas un tour de la conversation tient au mode et non à cette exécution : cela n'est pas dit ici.
manifest-needs-a-task = /manifest prend la tâche à planifier, comme /manifest résume la documentation
manifest-began = la tâche entière est planifiée d'abord ; la session attend ici jusqu'à la fin de l'exécution
manifest-ended-unexpectedly = l'exécution s'est terminée de façon inattendue
manifest-failed = l'exécution s'est arrêtée : { $problem }
manifest-recorded = enregistré sous { $id } ; à relire avec bravebot --resume { $id }


## L'écran d'accueil

opening-confinement = confinement { $level }
opening-invitation = Posez une question sur cet espace de travail.


## Ce qu'un tour a fait, dans les mots qui ouvrent une ligne de transcription

verb-read-file = Lire
verb-list-files = Lister
verb-search = Chercher
verb-lsp = Consulter
verb-write-file = Écrire
verb-edit-file = Modifier
verb-todo-write = Planifier
verb-spawn-processor = Processeur isolé
verb-load-skill = Compétence
verb-ask-user = Demander
verb-run = Exécuter
verb-read-output = Lire la sortie
verb-vet-content = Vérifier
verb-fetch-url = Récupérer
verb-job-output = Tâche
verb-spawn-agent = Déléguer
verb-schedule-next = Programmer
verb-watch-file = Surveiller
verb-unknown = Outil


## Où a atterri ce qu'un appel a produit, dit en fin de ligne

landed-in-the-planner = lu dans le contexte du planificateur
landed-quarantined =
    pas dans le contexte du planificateur ; seul un processeur isolé peut être envoyé le lire
landed-reserved = lu par rien : seul son nom est connu
reach-not-the-planner =
    pas dans le contexte du planificateur ; un processeur peut être envoyé le lire
reach-no-model = dans le contexte d'aucun modèle : rien ne peut être envoyé lire ceci

# How many calls a delegate has made, where its block shows only the last few.
delegate-more-calls = { $count } appels jusqu'ici

## Regarder ce que fait un delegue

# Le pied de page de la vue d'un delegue. Le genre et le numero sont les mots du pilote, jamais
# ceux du modele.
watching-footer = delegue { $kind } { $number }
watching-working = au travail
watching-answered = a repondu
watching-failed = n'a pas termine
watching-position = { $at } sur { $total }
watching-keys = q ferme  ·  n / p un autre delegue
watching-keys-one = q ferme
watching-keys-back = q revient  ·  n / p un autre delegue
watching-nothing-yet = rien pour l'instant
# La liste de tous les delegues lances par ce tour, la session au-dessus d'eux.
watching-list-title = delegues
watching-list-keys = haut / bas deplace  ·  entree ouvre  ·  q ferme
# La premiere ligne de la liste : la conversation d'ou viennent les delegues.
watching-list-session = session
watching-list-session-detail = retour a la conversation
watching-calls = { $count ->
    [one] { $count } appel
   *[other] { $count } appels
    }
# Dit sur la ligne du bas une fois que la vue a quelque chose a ouvrir, la seule ligne qui survit
# au tour qui l'a dessinee. Le compte y est car une touche sans rien derriere ne vaut pas la
# peine. Delegues et commandes sont comptes ensemble, une seule touche ouvrant la liste des deux.
watching-hint = { $chord } { $count } a ouvrir

# Vérifications indicatives affichées uniquement dans un dépôt de sources de Bravebot.
doctor-development = environnement de développement { $path }
doctor-agents-ok = OK (lien vers agents/AGENTS.md)
doctor-agents-copy-ok = OK (copie Windows de agents/AGENTS.md)
doctor-agents-missing = absent ; lancez `python3 agents/setup.py link` à la racine du dépôt
doctor-agents-broken = lien rompu ou illisible ; lancez `python3 agents/setup.py link` à la racine du dépôt
doctor-agents-wrong = le lien pointe vers la mauvaise cible ; lancez `python3 agents/setup.py link` à la racine du dépôt
doctor-agents-copy-stale = copie Windows obsolète ou illisible ; lancez `python3 agents/setup.py link` à la racine du dépôt
doctor-agents-conflict = conflit : résolvez d'abord le fichier ou le répertoire existant, puis lancez `python3 agents/setup.py link` à la racine du dépôt
doctor-agents-unreadable = impossible d'inspecter ce chemin ; résolvez d'abord ses permissions d'accès
doctor-direnv-ok = disponible dans le PATH
doctor-direnv-missing = introuvable dans le PATH ; consultez https://direnv.net/ ou lancez `brew install direnv`
