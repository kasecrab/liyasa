//! The theme's interface strings in the languages Liyasa ships (CM-105).
//!
//! `liyasa_theme::strings::Strings` owns which strings exist and their English
//! wording (THM-40); this owns what each one says in the other languages. The
//! split is deliberate: the theme is the surface a partial may eject and the
//! catalogue is content, and a locale must not mean a fork of either.
//!
//! An operator still wins. [`resolve`] applies four layers in order — the
//! theme's English defaults, the shipped catalogue for the locale,
//! `theme/strings.json`, then `theme/strings.<locale>.json` — so overriding one
//! word in one language never costs the rest of the translation.
//!
//! [`NOT_TRANSLATED`] is the one key here that the theme does not declare: it
//! belongs to CM-102's fallback notice rather than to the theme's chrome, and
//! it is fetched with [`not_translated`] rather than through `Strings`.

use std::collections::BTreeMap;

use liyasa_theme::strings::Strings;

/// CM-102's "not yet translated" notice. Not a `Strings` key: it is raised by
/// the build's locale fallback, not written into a theme partial.
pub const NOT_TRANSLATED: &str = "notTranslated";

/// One shipped language.
pub struct Catalog {
    /// The BCP 47 tag this catalogue is written in.
    pub code: &'static str,
    /// What the language calls itself, for the switcher's default label.
    pub endonym: &'static str,
    pub entries: &'static [(&'static str, &'static str)],
}

/// Every language Liyasa ships interface strings for, in tag order.
///
/// CM-105 asks for at least twelve. The English row is here so that the
/// catalogue is uniform and [`not_translated`] has an answer for the default
/// locale; its wording is the theme's own and is not a translation of anything.
pub const CATALOGS: &[Catalog] = &[
    Catalog {
        code: "en",
        endonym: "English",
        entries: EN,
    },
    Catalog {
        code: "de",
        endonym: "Deutsch",
        entries: DE,
    },
    Catalog {
        code: "es",
        endonym: "Español",
        entries: ES,
    },
    Catalog {
        code: "fr",
        endonym: "Français",
        entries: FR,
    },
    Catalog {
        code: "it",
        endonym: "Italiano",
        entries: IT,
    },
    Catalog {
        code: "ja",
        endonym: "日本語",
        entries: JA,
    },
    Catalog {
        code: "ko",
        endonym: "한국어",
        entries: KO,
    },
    Catalog {
        code: "nl",
        endonym: "Nederlands",
        entries: NL,
    },
    Catalog {
        code: "pt-BR",
        endonym: "Português do Brasil",
        entries: PT_BR,
    },
    Catalog {
        code: "ru",
        endonym: "Русский",
        entries: RU,
    },
    Catalog {
        code: "tr",
        endonym: "Türkçe",
        entries: TR,
    },
    Catalog {
        code: "zh-CN",
        endonym: "简体中文",
        entries: ZH_CN,
    },
    Catalog {
        code: "zh-TW",
        endonym: "繁體中文",
        entries: ZH_TW,
    },
];

/// The catalogue for a locale tag.
///
/// An exact tag wins. Failing that the primary subtag is tried, so `de-AT`
/// reads the German catalogue and `pt` reads `pt-BR` — the shipped variant of
/// that language rather than nothing at all. A tag no shipped language shares a
/// primary subtag with has no catalogue, and the caller keeps the theme's
/// English.
pub fn catalog(code: &str) -> Option<&'static Catalog> {
    if let Some(exact) = CATALOGS
        .iter()
        .find(|catalog| catalog.code.eq_ignore_ascii_case(code))
    {
        return Some(exact);
    }
    let primary = primary_subtag(code);
    CATALOGS
        .iter()
        .find(|catalog| primary_subtag(catalog.code).eq_ignore_ascii_case(primary))
}

fn primary_subtag(code: &str) -> &str {
    code.split(['-', '_']).next().unwrap_or(code)
}

/// The tags the catalogue ships, for the build report and for `liyasa verify`.
pub fn shipped() -> Vec<&'static str> {
    CATALOGS.iter().map(|catalog| catalog.code).collect()
}

/// What a language calls itself, which is the switcher label CM-101 wants when
/// the config declares none.
pub fn endonym(code: &str) -> Option<&'static str> {
    catalog(code).map(|catalog| catalog.endonym)
}

/// CM-102's notice in the reader's own language, falling back to English.
pub fn not_translated(code: &str) -> &'static str {
    catalog(code)
        .and_then(|catalog| lookup(catalog, NOT_TRANSLATED))
        .unwrap_or("This page is not yet translated.")
}

fn lookup(catalog: &'static Catalog, key: &str) -> Option<&'static str> {
    catalog
        .entries
        .iter()
        .find(|(name, _)| *name == key)
        .map(|(_, value)| *value)
}

/// The shipped translation for one locale as `Strings` overrides.
pub fn overrides(code: &str) -> BTreeMap<String, String> {
    let Some(catalog) = catalog(code) else {
        return BTreeMap::new();
    };
    catalog
        .entries
        .iter()
        .filter(|(key, _)| *key != NOT_TRANSLATED)
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect()
}

/// The four layers of CM-105, in order, with the keys no layer recognized.
///
/// `site` is `theme/strings.json` and `per_locale` is
/// `theme/strings.<locale>.json`; either may be empty. Only the operator's keys
/// are reported back — a shipped catalogue that had drifted from the theme
/// would be a failing test here rather than a typo in someone's config.
pub fn resolve(
    code: &str,
    site: &BTreeMap<String, String>,
    per_locale: &BTreeMap<String, String>,
) -> (Strings, Vec<String>) {
    let (strings, _) = Strings::default().with_overrides(&overrides(code));
    let (strings, mut unknown) = strings.with_overrides(site);
    let (strings, rest) = strings.with_overrides(per_locale);
    unknown.extend(rest);
    unknown.sort();
    unknown.dedup();
    (strings, unknown)
}

const EN: &[(&str, &str)] = &[
    ("skipToContent", "Skip to content"),
    ("search", "Search"),
    ("searchPlaceholder", "Search the documentation"),
    ("searchEmpty", "No results"),
    ("searchHint", "Press Enter to open, Escape to close"),
    ("askAi", "Ask AI"),
    ("assistant", "Assistant"),
    ("menu", "Menu"),
    ("close", "Close"),
    ("onThisPage", "On this page"),
    ("backToTop", "Back to top"),
    ("previous", "Previous"),
    ("next", "Next"),
    ("lastUpdated", "Last updated"),
    ("editPage", "Edit this page"),
    ("suggestEdit", "Suggest an edit"),
    ("feedbackQuestion", "Was this page helpful?"),
    ("feedbackYes", "Yes"),
    ("feedbackNo", "No"),
    ("feedbackThanks", "Thank you for the feedback"),
    ("copy", "Copy"),
    ("copied", "Copied"),
    ("copyPage", "Copy page as Markdown"),
    ("viewMarkdown", "View as Markdown"),
    ("copyMcpUrl", "Copy MCP server URL"),
    ("downloadPdf", "Download as PDF"),
    ("pageActions", "Page actions"),
    ("openIn", "Open in"),
    ("toggleTheme", "Toggle dark mode"),
    ("dismissBanner", "Dismiss"),
    ("version", "Version"),
    ("language", "Language"),
    ("notFoundTitle", "Page not found"),
    (
        "notFoundDescription",
        "The page you are looking for does not exist.",
    ),
    ("notFoundHome", "Back to the documentation"),
    ("builtWith", "Built with Liyasa"),
    (NOT_TRANSLATED, "This page is not yet translated."),
];

const DE: &[(&str, &str)] = &[
    ("skipToContent", "Zum Inhalt springen"),
    ("search", "Suchen"),
    ("searchPlaceholder", "Dokumentation durchsuchen"),
    ("searchEmpty", "Keine Ergebnisse"),
    (
        "searchHint",
        "Eingabetaste zum Öffnen, Escape zum Schließen",
    ),
    ("askAi", "KI fragen"),
    ("assistant", "Assistent"),
    ("menu", "Menü"),
    ("close", "Schließen"),
    ("onThisPage", "Auf dieser Seite"),
    ("backToTop", "Nach oben"),
    ("previous", "Zurück"),
    ("next", "Weiter"),
    ("lastUpdated", "Zuletzt aktualisiert"),
    ("editPage", "Diese Seite bearbeiten"),
    ("suggestEdit", "Änderung vorschlagen"),
    ("feedbackQuestion", "War diese Seite hilfreich?"),
    ("feedbackYes", "Ja"),
    ("feedbackNo", "Nein"),
    ("feedbackThanks", "Danke für das Feedback"),
    ("copy", "Kopieren"),
    ("copied", "Kopiert"),
    ("copyPage", "Seite als Markdown kopieren"),
    ("viewMarkdown", "Als Markdown anzeigen"),
    ("copyMcpUrl", "MCP-Server-URL kopieren"),
    ("downloadPdf", "Als PDF herunterladen"),
    ("pageActions", "Seitenaktionen"),
    ("openIn", "Öffnen in"),
    ("toggleTheme", "Dunkelmodus umschalten"),
    ("dismissBanner", "Schließen"),
    ("version", "Version"),
    ("language", "Sprache"),
    ("notFoundTitle", "Seite nicht gefunden"),
    ("notFoundDescription", "Die gesuchte Seite existiert nicht."),
    ("notFoundHome", "Zurück zur Dokumentation"),
    ("builtWith", "Erstellt mit Liyasa"),
    (NOT_TRANSLATED, "Diese Seite ist noch nicht übersetzt."),
];

const ES: &[(&str, &str)] = &[
    ("skipToContent", "Saltar al contenido"),
    ("search", "Buscar"),
    ("searchPlaceholder", "Buscar en la documentación"),
    ("searchEmpty", "Sin resultados"),
    ("searchHint", "Pulsa Intro para abrir, Escape para cerrar"),
    ("askAi", "Preguntar a la IA"),
    ("assistant", "Asistente"),
    ("menu", "Menú"),
    ("close", "Cerrar"),
    ("onThisPage", "En esta página"),
    ("backToTop", "Volver arriba"),
    ("previous", "Anterior"),
    ("next", "Siguiente"),
    ("lastUpdated", "Última actualización"),
    ("editPage", "Editar esta página"),
    ("suggestEdit", "Sugerir un cambio"),
    ("feedbackQuestion", "¿Te ha resultado útil esta página?"),
    ("feedbackYes", "Sí"),
    ("feedbackNo", "No"),
    ("feedbackThanks", "Gracias por tu opinión"),
    ("copy", "Copiar"),
    ("copied", "Copiado"),
    ("copyPage", "Copiar la página como Markdown"),
    ("viewMarkdown", "Ver como Markdown"),
    ("copyMcpUrl", "Copiar la URL del servidor MCP"),
    ("downloadPdf", "Descargar en PDF"),
    ("pageActions", "Acciones de la página"),
    ("openIn", "Abrir en"),
    ("toggleTheme", "Cambiar al modo oscuro"),
    ("dismissBanner", "Cerrar"),
    ("version", "Versión"),
    ("language", "Idioma"),
    ("notFoundTitle", "Página no encontrada"),
    ("notFoundDescription", "La página que buscas no existe."),
    ("notFoundHome", "Volver a la documentación"),
    ("builtWith", "Creado con Liyasa"),
    (NOT_TRANSLATED, "Esta página aún no está traducida."),
];

const FR: &[(&str, &str)] = &[
    ("skipToContent", "Aller au contenu"),
    ("search", "Rechercher"),
    ("searchPlaceholder", "Rechercher dans la documentation"),
    ("searchEmpty", "Aucun résultat"),
    ("searchHint", "Entrée pour ouvrir, Échap pour fermer"),
    ("askAi", "Demander à l'IA"),
    ("assistant", "Assistant"),
    ("menu", "Menu"),
    ("close", "Fermer"),
    ("onThisPage", "Sur cette page"),
    ("backToTop", "Haut de page"),
    ("previous", "Précédent"),
    ("next", "Suivant"),
    ("lastUpdated", "Dernière mise à jour"),
    ("editPage", "Modifier cette page"),
    ("suggestEdit", "Proposer une modification"),
    ("feedbackQuestion", "Cette page vous a-t-elle été utile ?"),
    ("feedbackYes", "Oui"),
    ("feedbackNo", "Non"),
    ("feedbackThanks", "Merci pour votre retour"),
    ("copy", "Copier"),
    ("copied", "Copié"),
    ("copyPage", "Copier la page en Markdown"),
    ("viewMarkdown", "Afficher en Markdown"),
    ("copyMcpUrl", "Copier l'URL du serveur MCP"),
    ("downloadPdf", "Télécharger en PDF"),
    ("pageActions", "Actions de la page"),
    ("openIn", "Ouvrir dans"),
    ("toggleTheme", "Basculer en mode sombre"),
    ("dismissBanner", "Fermer"),
    ("version", "Version"),
    ("language", "Langue"),
    ("notFoundTitle", "Page introuvable"),
    (
        "notFoundDescription",
        "La page que vous cherchez n'existe pas.",
    ),
    ("notFoundHome", "Retour à la documentation"),
    ("builtWith", "Créé avec Liyasa"),
    (NOT_TRANSLATED, "Cette page n'est pas encore traduite."),
];

const IT: &[(&str, &str)] = &[
    ("skipToContent", "Vai al contenuto"),
    ("search", "Cerca"),
    ("searchPlaceholder", "Cerca nella documentazione"),
    ("searchEmpty", "Nessun risultato"),
    ("searchHint", "Invio per aprire, Esc per chiudere"),
    ("askAi", "Chiedi all'IA"),
    ("assistant", "Assistente"),
    ("menu", "Menu"),
    ("close", "Chiudi"),
    ("onThisPage", "In questa pagina"),
    ("backToTop", "Torna su"),
    ("previous", "Precedente"),
    ("next", "Successivo"),
    ("lastUpdated", "Ultimo aggiornamento"),
    ("editPage", "Modifica questa pagina"),
    ("suggestEdit", "Proponi una modifica"),
    ("feedbackQuestion", "Questa pagina è stata utile?"),
    ("feedbackYes", "Sì"),
    ("feedbackNo", "No"),
    ("feedbackThanks", "Grazie per il feedback"),
    ("copy", "Copia"),
    ("copied", "Copiato"),
    ("copyPage", "Copia la pagina come Markdown"),
    ("viewMarkdown", "Visualizza come Markdown"),
    ("copyMcpUrl", "Copia l'URL del server MCP"),
    ("downloadPdf", "Scarica in PDF"),
    ("pageActions", "Azioni della pagina"),
    ("openIn", "Apri in"),
    ("toggleTheme", "Attiva o disattiva la modalità scura"),
    ("dismissBanner", "Chiudi"),
    ("version", "Versione"),
    ("language", "Lingua"),
    ("notFoundTitle", "Pagina non trovata"),
    ("notFoundDescription", "La pagina che cerchi non esiste."),
    ("notFoundHome", "Torna alla documentazione"),
    ("builtWith", "Creato con Liyasa"),
    (NOT_TRANSLATED, "Questa pagina non è ancora tradotta."),
];

const JA: &[(&str, &str)] = &[
    ("skipToContent", "本文へスキップ"),
    ("search", "検索"),
    ("searchPlaceholder", "ドキュメントを検索"),
    ("searchEmpty", "結果がありません"),
    ("searchHint", "Enter で開く、Esc で閉じる"),
    ("askAi", "AI に質問"),
    ("assistant", "アシスタント"),
    ("menu", "メニュー"),
    ("close", "閉じる"),
    ("onThisPage", "このページの内容"),
    ("backToTop", "先頭に戻る"),
    ("previous", "前へ"),
    ("next", "次へ"),
    ("lastUpdated", "最終更新"),
    ("editPage", "このページを編集"),
    ("suggestEdit", "修正を提案"),
    ("feedbackQuestion", "このページは役に立ちましたか？"),
    ("feedbackYes", "はい"),
    ("feedbackNo", "いいえ"),
    ("feedbackThanks", "フィードバックをありがとうございます"),
    ("copy", "コピー"),
    ("copied", "コピーしました"),
    ("copyPage", "ページを Markdown としてコピー"),
    ("viewMarkdown", "Markdown で表示"),
    ("copyMcpUrl", "MCP サーバーの URL をコピー"),
    ("downloadPdf", "PDF をダウンロード"),
    ("pageActions", "ページの操作"),
    ("openIn", "次で開く"),
    ("toggleTheme", "ダークモードを切り替え"),
    ("dismissBanner", "閉じる"),
    ("version", "バージョン"),
    ("language", "言語"),
    ("notFoundTitle", "ページが見つかりません"),
    ("notFoundDescription", "お探しのページは存在しません。"),
    ("notFoundHome", "ドキュメントに戻る"),
    ("builtWith", "Liyasa で作成"),
    (NOT_TRANSLATED, "このページはまだ翻訳されていません。"),
];

const KO: &[(&str, &str)] = &[
    ("skipToContent", "본문으로 건너뛰기"),
    ("search", "검색"),
    ("searchPlaceholder", "문서 검색"),
    ("searchEmpty", "결과 없음"),
    ("searchHint", "Enter로 열기, Esc로 닫기"),
    ("askAi", "AI에게 묻기"),
    ("assistant", "어시스턴트"),
    ("menu", "메뉴"),
    ("close", "닫기"),
    ("onThisPage", "이 페이지의 내용"),
    ("backToTop", "맨 위로"),
    ("previous", "이전"),
    ("next", "다음"),
    ("lastUpdated", "마지막 업데이트"),
    ("editPage", "이 페이지 편집"),
    ("suggestEdit", "수정 제안"),
    ("feedbackQuestion", "이 페이지가 도움이 되었나요?"),
    ("feedbackYes", "예"),
    ("feedbackNo", "아니요"),
    ("feedbackThanks", "의견 감사합니다"),
    ("copy", "복사"),
    ("copied", "복사됨"),
    ("copyPage", "페이지를 Markdown으로 복사"),
    ("viewMarkdown", "Markdown으로 보기"),
    ("copyMcpUrl", "MCP 서버 URL 복사"),
    ("downloadPdf", "PDF로 다운로드"),
    ("pageActions", "페이지 작업"),
    ("openIn", "다음에서 열기"),
    ("toggleTheme", "다크 모드 전환"),
    ("dismissBanner", "닫기"),
    ("version", "버전"),
    ("language", "언어"),
    ("notFoundTitle", "페이지를 찾을 수 없습니다"),
    (
        "notFoundDescription",
        "찾으시는 페이지가 존재하지 않습니다.",
    ),
    ("notFoundHome", "문서로 돌아가기"),
    ("builtWith", "Liyasa로 제작"),
    (NOT_TRANSLATED, "이 페이지는 아직 번역되지 않았습니다."),
];

const NL: &[(&str, &str)] = &[
    ("skipToContent", "Naar de inhoud"),
    ("search", "Zoeken"),
    ("searchPlaceholder", "Zoek in de documentatie"),
    ("searchEmpty", "Geen resultaten"),
    ("searchHint", "Enter om te openen, Escape om te sluiten"),
    ("askAi", "Vraag het de AI"),
    ("assistant", "Assistent"),
    ("menu", "Menu"),
    ("close", "Sluiten"),
    ("onThisPage", "Op deze pagina"),
    ("backToTop", "Terug naar boven"),
    ("previous", "Vorige"),
    ("next", "Volgende"),
    ("lastUpdated", "Laatst bijgewerkt"),
    ("editPage", "Deze pagina bewerken"),
    ("suggestEdit", "Wijziging voorstellen"),
    ("feedbackQuestion", "Was deze pagina nuttig?"),
    ("feedbackYes", "Ja"),
    ("feedbackNo", "Nee"),
    ("feedbackThanks", "Bedankt voor je feedback"),
    ("copy", "Kopiëren"),
    ("copied", "Gekopieerd"),
    ("copyPage", "Pagina als Markdown kopiëren"),
    ("viewMarkdown", "Als Markdown bekijken"),
    ("copyMcpUrl", "MCP-server-URL kopiëren"),
    ("downloadPdf", "Downloaden als PDF"),
    ("pageActions", "Pagina-acties"),
    ("openIn", "Openen in"),
    ("toggleTheme", "Donkere modus wisselen"),
    ("dismissBanner", "Sluiten"),
    ("version", "Versie"),
    ("language", "Taal"),
    ("notFoundTitle", "Pagina niet gevonden"),
    (
        "notFoundDescription",
        "De pagina die je zoekt bestaat niet.",
    ),
    ("notFoundHome", "Terug naar de documentatie"),
    ("builtWith", "Gemaakt met Liyasa"),
    (NOT_TRANSLATED, "Deze pagina is nog niet vertaald."),
];

const PT_BR: &[(&str, &str)] = &[
    ("skipToContent", "Ir para o conteúdo"),
    ("search", "Pesquisar"),
    ("searchPlaceholder", "Pesquisar na documentação"),
    ("searchEmpty", "Nenhum resultado"),
    ("searchHint", "Enter para abrir, Esc para fechar"),
    ("askAi", "Perguntar à IA"),
    ("assistant", "Assistente"),
    ("menu", "Menu"),
    ("close", "Fechar"),
    ("onThisPage", "Nesta página"),
    ("backToTop", "Voltar ao topo"),
    ("previous", "Anterior"),
    ("next", "Próximo"),
    ("lastUpdated", "Última atualização"),
    ("editPage", "Editar esta página"),
    ("suggestEdit", "Sugerir uma alteração"),
    ("feedbackQuestion", "Esta página foi útil?"),
    ("feedbackYes", "Sim"),
    ("feedbackNo", "Não"),
    ("feedbackThanks", "Obrigado pelo feedback"),
    ("copy", "Copiar"),
    ("copied", "Copiado"),
    ("copyPage", "Copiar a página como Markdown"),
    ("viewMarkdown", "Ver como Markdown"),
    ("copyMcpUrl", "Copiar a URL do servidor MCP"),
    ("downloadPdf", "Baixar em PDF"),
    ("pageActions", "Ações da página"),
    ("openIn", "Abrir em"),
    ("toggleTheme", "Alternar o modo escuro"),
    ("dismissBanner", "Fechar"),
    ("version", "Versão"),
    ("language", "Idioma"),
    ("notFoundTitle", "Página não encontrada"),
    (
        "notFoundDescription",
        "A página que você procura não existe.",
    ),
    ("notFoundHome", "Voltar para a documentação"),
    ("builtWith", "Feito com Liyasa"),
    (NOT_TRANSLATED, "Esta página ainda não foi traduzida."),
];

const RU: &[(&str, &str)] = &[
    ("skipToContent", "Перейти к содержимому"),
    ("search", "Поиск"),
    ("searchPlaceholder", "Поиск по документации"),
    ("searchEmpty", "Ничего не найдено"),
    ("searchHint", "Enter — открыть, Escape — закрыть"),
    ("askAi", "Спросить ИИ"),
    ("assistant", "Ассистент"),
    ("menu", "Меню"),
    ("close", "Закрыть"),
    ("onThisPage", "На этой странице"),
    ("backToTop", "Наверх"),
    ("previous", "Назад"),
    ("next", "Далее"),
    ("lastUpdated", "Последнее обновление"),
    ("editPage", "Редактировать страницу"),
    ("suggestEdit", "Предложить правку"),
    ("feedbackQuestion", "Эта страница была полезной?"),
    ("feedbackYes", "Да"),
    ("feedbackNo", "Нет"),
    ("feedbackThanks", "Спасибо за отзыв"),
    ("copy", "Копировать"),
    ("copied", "Скопировано"),
    ("copyPage", "Копировать страницу как Markdown"),
    ("viewMarkdown", "Открыть как Markdown"),
    ("copyMcpUrl", "Копировать URL сервера MCP"),
    ("downloadPdf", "Скачать в PDF"),
    ("pageActions", "Действия со страницей"),
    ("openIn", "Открыть в"),
    ("toggleTheme", "Переключить тёмную тему"),
    ("dismissBanner", "Закрыть"),
    ("version", "Версия"),
    ("language", "Язык"),
    ("notFoundTitle", "Страница не найдена"),
    (
        "notFoundDescription",
        "Страница, которую вы ищете, не существует.",
    ),
    ("notFoundHome", "Вернуться к документации"),
    ("builtWith", "Создано с Liyasa"),
    (NOT_TRANSLATED, "Эта страница ещё не переведена."),
];

const TR: &[(&str, &str)] = &[
    ("skipToContent", "İçeriğe geç"),
    ("search", "Ara"),
    ("searchPlaceholder", "Belgelerde ara"),
    ("searchEmpty", "Sonuç yok"),
    ("searchHint", "Açmak için Enter, kapatmak için Esc"),
    ("askAi", "Yapay zekâya sor"),
    ("assistant", "Asistan"),
    ("menu", "Menü"),
    ("close", "Kapat"),
    ("onThisPage", "Bu sayfada"),
    ("backToTop", "Başa dön"),
    ("previous", "Önceki"),
    ("next", "Sonraki"),
    ("lastUpdated", "Son güncelleme"),
    ("editPage", "Bu sayfayı düzenle"),
    ("suggestEdit", "Değişiklik öner"),
    ("feedbackQuestion", "Bu sayfa yardımcı oldu mu?"),
    ("feedbackYes", "Evet"),
    ("feedbackNo", "Hayır"),
    ("feedbackThanks", "Geri bildiriminiz için teşekkürler"),
    ("copy", "Kopyala"),
    ("copied", "Kopyalandı"),
    ("copyPage", "Sayfayı Markdown olarak kopyala"),
    ("viewMarkdown", "Markdown olarak görüntüle"),
    ("copyMcpUrl", "MCP sunucu URL'sini kopyala"),
    ("downloadPdf", "PDF olarak indir"),
    ("pageActions", "Sayfa işlemleri"),
    ("openIn", "Şununla aç"),
    ("toggleTheme", "Koyu modu aç veya kapat"),
    ("dismissBanner", "Kapat"),
    ("version", "Sürüm"),
    ("language", "Dil"),
    ("notFoundTitle", "Sayfa bulunamadı"),
    ("notFoundDescription", "Aradığınız sayfa mevcut değil."),
    ("notFoundHome", "Belgelere dön"),
    ("builtWith", "Liyasa ile oluşturuldu"),
    (NOT_TRANSLATED, "Bu sayfa henüz çevrilmedi."),
];

const ZH_CN: &[(&str, &str)] = &[
    ("skipToContent", "跳到主要内容"),
    ("search", "搜索"),
    ("searchPlaceholder", "搜索文档"),
    ("searchEmpty", "没有结果"),
    ("searchHint", "按 Enter 打开，按 Esc 关闭"),
    ("askAi", "询问 AI"),
    ("assistant", "助手"),
    ("menu", "菜单"),
    ("close", "关闭"),
    ("onThisPage", "本页内容"),
    ("backToTop", "回到顶部"),
    ("previous", "上一页"),
    ("next", "下一页"),
    ("lastUpdated", "最后更新"),
    ("editPage", "编辑此页"),
    ("suggestEdit", "建议修改"),
    ("feedbackQuestion", "这个页面对你有帮助吗？"),
    ("feedbackYes", "有"),
    ("feedbackNo", "没有"),
    ("feedbackThanks", "感谢你的反馈"),
    ("copy", "复制"),
    ("copied", "已复制"),
    ("copyPage", "复制页面为 Markdown"),
    ("viewMarkdown", "以 Markdown 查看"),
    ("copyMcpUrl", "复制 MCP 服务器地址"),
    ("downloadPdf", "下载 PDF"),
    ("pageActions", "页面操作"),
    ("openIn", "在以下应用中打开"),
    ("toggleTheme", "切换深色模式"),
    ("dismissBanner", "关闭"),
    ("version", "版本"),
    ("language", "语言"),
    ("notFoundTitle", "找不到页面"),
    ("notFoundDescription", "你要找的页面不存在。"),
    ("notFoundHome", "返回文档"),
    ("builtWith", "由 Liyasa 构建"),
    (NOT_TRANSLATED, "此页面尚未翻译。"),
];

const ZH_TW: &[(&str, &str)] = &[
    ("skipToContent", "跳至主要內容"),
    ("search", "搜尋"),
    ("searchPlaceholder", "搜尋文件"),
    ("searchEmpty", "沒有結果"),
    ("searchHint", "按 Enter 開啟，按 Esc 關閉"),
    ("askAi", "詢問 AI"),
    ("assistant", "助理"),
    ("menu", "選單"),
    ("close", "關閉"),
    ("onThisPage", "本頁內容"),
    ("backToTop", "回到頂端"),
    ("previous", "上一頁"),
    ("next", "下一頁"),
    ("lastUpdated", "最後更新"),
    ("editPage", "編輯此頁"),
    ("suggestEdit", "建議修改"),
    ("feedbackQuestion", "這個頁面對你有幫助嗎？"),
    ("feedbackYes", "有"),
    ("feedbackNo", "沒有"),
    ("feedbackThanks", "感謝你的意見"),
    ("copy", "複製"),
    ("copied", "已複製"),
    ("copyPage", "複製頁面為 Markdown"),
    ("viewMarkdown", "以 Markdown 檢視"),
    ("copyMcpUrl", "複製 MCP 伺服器網址"),
    ("downloadPdf", "下載 PDF"),
    ("pageActions", "頁面操作"),
    ("openIn", "在以下應用程式中開啟"),
    ("toggleTheme", "切換深色模式"),
    ("dismissBanner", "關閉"),
    ("version", "版本"),
    ("language", "語言"),
    ("notFoundTitle", "找不到頁面"),
    ("notFoundDescription", "你要找的頁面不存在。"),
    ("notFoundHome", "返回文件"),
    ("builtWith", "以 Liyasa 建置"),
    (NOT_TRANSLATED, "此頁面尚未翻譯。"),
];

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn cm_105_ships_at_least_twelve_languages_beyond_the_default() {
        let translated = CATALOGS.len() - 1;
        assert!(
            translated >= 12,
            "CM-105 asks for twelve; the catalogue has {translated}"
        );
    }

    /// The catalogue and `liyasa_theme::strings` are two files that have to
    /// agree. A key added to the theme and not translated here would otherwise
    /// reach a German reader in English with nothing to say so.
    #[test]
    fn every_catalogue_covers_every_theme_key_and_invents_none() {
        let theme: BTreeSet<&str> = Strings::KEYS.iter().copied().collect();
        for catalog in CATALOGS {
            let keys: BTreeSet<&str> = catalog.entries.iter().map(|(key, _)| *key).collect();
            assert_eq!(
                keys.len(),
                catalog.entries.len(),
                "`{}` repeats a key",
                catalog.code
            );
            let missing: Vec<&&str> = theme.difference(&keys).collect();
            assert!(
                missing.is_empty(),
                "`{}` is missing {missing:?}",
                catalog.code
            );
            let extra: Vec<&str> = keys
                .difference(&theme)
                .copied()
                .filter(|key| *key != NOT_TRANSLATED)
                .collect();
            assert!(
                extra.is_empty(),
                "`{}` translates {extra:?}, which the theme does not declare",
                catalog.code
            );
        }
    }

    #[test]
    fn no_translation_is_left_empty_or_left_in_english() {
        let english: BTreeMap<&str, &str> = EN.iter().copied().collect();
        for catalog in CATALOGS.iter().filter(|catalog| catalog.code != "en") {
            for (key, value) in catalog.entries {
                assert!(!value.is_empty(), "`{}` has no `{key}`", catalog.code);
                // Proper nouns and the two single-letter answers are the same
                // word in several of these languages; everything else that
                // matches English is an untranslated stub.
                let shared = ["assistant", "menu", "version", "copy", "feedbackNo"];
                if shared.contains(key) {
                    continue;
                }
                assert_ne!(
                    english.get(key),
                    Some(value),
                    "`{}` leaves `{key}` in English",
                    catalog.code
                );
            }
        }
    }

    #[test]
    fn a_region_tag_reads_its_languages_catalogue() {
        assert_eq!(catalog("de-AT").map(|c| c.code), Some("de"));
        assert_eq!(catalog("pt").map(|c| c.code), Some("pt-BR"));
        assert_eq!(catalog("zh").map(|c| c.code), Some("zh-CN"));
        assert_eq!(catalog("pt-BR").map(|c| c.code), Some("pt-BR"));
        assert_eq!(catalog("cy").map(|c| c.code), None);
    }

    #[test]
    fn the_notice_is_in_the_readers_language_and_english_when_it_is_not_shipped() {
        assert_eq!(not_translated("ja"), "このページはまだ翻訳されていません。");
        assert_eq!(
            not_translated("cy"),
            "This page is not yet translated.",
            "an unshipped language keeps the English notice"
        );
    }

    #[test]
    fn the_notice_is_not_offered_as_a_theme_string() {
        assert!(
            !overrides("de").contains_key(NOT_TRANSLATED),
            "the theme has no such key and would report it as a typo"
        );
        let (_, unknown) = resolve("de", &BTreeMap::new(), &BTreeMap::new());
        assert!(unknown.is_empty(), "{unknown:?}");
    }

    #[test]
    fn a_shipped_translation_is_applied_over_the_themes_english() {
        let (strings, unknown) = resolve("de", &BTreeMap::new(), &BTreeMap::new());
        assert_eq!(strings.on_this_page, "Auf dieser Seite");
        assert_eq!(strings.feedback_question, "War diese Seite hilfreich?");
        assert!(unknown.is_empty());
    }

    #[test]
    fn an_operator_overrides_one_word_without_losing_the_translation() {
        let site = BTreeMap::from([("builtWith".to_owned(), "Docs by Acme".to_owned())]);
        let per_locale = BTreeMap::from([("askAi".to_owned(), "Acme fragen".to_owned())]);
        let (strings, unknown) = resolve("de", &site, &per_locale);
        assert_eq!(strings.built_with, "Docs by Acme");
        assert_eq!(strings.ask_ai, "Acme fragen");
        assert_eq!(
            strings.on_this_page, "Auf dieser Seite",
            "the rest of the German is untouched"
        );
        assert!(unknown.is_empty());
    }

    #[test]
    fn the_per_locale_file_wins_over_the_site_wide_one() {
        let site = BTreeMap::from([("search".to_owned(), "Find".to_owned())]);
        let per_locale = BTreeMap::from([("search".to_owned(), "Finden".to_owned())]);
        let (strings, _) = resolve("de", &site, &per_locale);
        assert_eq!(strings.search, "Finden");
    }

    #[test]
    fn an_operators_typo_is_reported_from_either_file() {
        let site = BTreeMap::from([("bultWith".to_owned(), "x".to_owned())]);
        let per_locale = BTreeMap::from([("onThisPge".to_owned(), "y".to_owned())]);
        let (_, unknown) = resolve("de", &site, &per_locale);
        assert_eq!(unknown, vec!["bultWith".to_owned(), "onThisPge".to_owned()]);
    }

    #[test]
    fn an_unshipped_locale_keeps_the_themes_english_rather_than_failing() {
        let (strings, unknown) = resolve("cy", &BTreeMap::new(), &BTreeMap::new());
        assert_eq!(strings.on_this_page, "On this page");
        assert!(unknown.is_empty());
    }

    #[test]
    fn every_language_names_itself_in_its_own_language() {
        assert_eq!(endonym("ja"), Some("日本語"));
        assert_eq!(endonym("pt"), Some("Português do Brasil"));
        assert_eq!(endonym("cy"), None);
        assert_eq!(shipped().len(), CATALOGS.len());
    }
}
