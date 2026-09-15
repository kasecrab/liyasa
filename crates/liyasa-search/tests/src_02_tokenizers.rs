//! SRC-02: one tokenizer module, shared by both indexes (PRD §12.2).

use liyasa_search::idx::tokenize::{self, Tokenizer};

fn terms(locale: &str, text: &str) -> Vec<String> {
    Tokenizer::for_locale(locale)
        .tokenize(text)
        .into_iter()
        .map(|t| t.text)
        .collect()
}

fn code_terms(text: &str) -> Vec<String> {
    tokenize::code(text).into_iter().map(|t| t.text).collect()
}

#[test]
fn english_words_are_lowercased_and_stemmed() {
    assert_eq!(
        terms("en", "The Connections are running"),
        ["the", "connect", "are", "run"]
    );
}

#[test]
fn german_uses_the_german_snowball_algorithm() {
    // `Verbindungen` stems to the same root as `Verbindung`, which is the
    // whole point of stemming the query the way the index was built.
    let plural = terms("de", "Verbindungen");
    let singular = terms("de", "Verbindung");
    assert_eq!(plural, singular, "plural and singular must share a stem");
    assert_eq!(plural.len(), 1);
}

#[test]
fn every_language_named_in_src_02_has_a_stemmer() {
    for locale in tokenize::STEMMED_LOCALES {
        assert!(
            tokenize::algorithm_for(locale).is_some(),
            "{locale} is named in SRC-02 but has no Snowball algorithm"
        );
    }
    assert_eq!(tokenize::STEMMED_LOCALES.len(), 17);
}

#[test]
fn a_region_subtag_still_finds_the_language() {
    assert_eq!(
        tokenize::algorithm_for("pt-BR"),
        tokenize::algorithm_for("pt")
    );
    assert_eq!(terms("en-GB", "running"), ["run"]);
}

#[test]
fn an_unsupported_language_indexes_unstemmed() {
    assert!(tokenize::algorithm_for("cs").is_none());
    assert_eq!(terms("cs", "Připojení Běží"), ["připojení", "běží"]);
}

#[test]
fn japanese_runs_become_bigrams_within_one_script() {
    // Han and Katakana are separate runs, as Lucene's CJK bigram filter has
    // them: `検索` is one Han bigram, `エンジン` three Katakana ones.
    assert_eq!(
        terms("ja", "検索エンジン"),
        ["検索", "エン", "ンジ", "ジン"]
    );
}

#[test]
fn a_lone_cjk_character_is_its_own_term() {
    assert_eq!(terms("zh", "本"), ["本"]);
    assert_eq!(terms("zh", "中文文档"), ["中文", "文文", "文档"]);
}

#[test]
fn korean_hangul_bigrams_too() {
    assert_eq!(terms("ko", "검색엔진"), ["검색", "색엔", "엔진"]);
}

#[test]
fn latin_and_cjk_mix_in_one_string() {
    assert_eq!(terms("ja", "API の検索"), ["api", "の", "検索"]);
}

#[test]
fn cjk_is_bigrammed_whatever_the_page_locale_says() {
    // A German page quoting Japanese must still be searchable in Japanese.
    assert_eq!(terms("de", "検索"), ["検索"]);
}

#[test]
fn code_splits_on_case() {
    assert_eq!(
        code_terms("getUserById"),
        ["getuserbyid", "get", "user", "by", "id"]
    );
}

#[test]
fn code_splits_on_punctuation_and_keeps_the_whole_identifier() {
    assert_eq!(
        code_terms("user_id"),
        ["user_id", "user", "id"].map(str::to_owned)
    );
    assert_eq!(code_terms("max-results"), ["max-results", "max", "results"]);
    assert_eq!(
        code_terms("liyasa.search.mode"),
        ["liyasa.search.mode", "liyasa", "search", "mode"]
    );
}

#[test]
fn code_keeps_acronyms_whole() {
    assert_eq!(code_terms("HTTPServer"), ["httpserver", "http", "server"]);
    assert_eq!(
        code_terms("parseJSONBody"),
        ["parsejsonbody", "parse", "json", "body"]
    );
}

#[test]
fn code_splits_letters_from_digits() {
    assert_eq!(
        code_terms("utf8Decode"),
        ["utf8decode", "utf", "8", "decode"]
    );
}

#[test]
fn a_single_word_identifier_is_not_repeated() {
    assert_eq!(code_terms("search"), ["search"]);
}

#[test]
fn code_is_never_stemmed() {
    // `running` is a plausible method name; stemming it would stop
    // `running` from matching itself in the code field.
    assert_eq!(code_terms("running"), ["running"]);
}

#[test]
fn positions_are_consecutive_so_phrases_match() {
    let tokens = Tokenizer::for_locale("en").tokenize("alpha beta gamma");
    assert_eq!(
        tokens.iter().map(|t| t.position).collect::<Vec<_>>(),
        [0, 1, 2]
    );
}

#[test]
fn the_whole_identifier_shares_the_first_parts_position() {
    let tokens = tokenize::code("getUser next");
    let whole = &tokens[0];
    let first = &tokens[1];
    assert_eq!(whole.text, "getuser");
    assert_eq!(whole.position, first.position);
    assert_eq!(tokens.last().map(|t| t.text.as_str()), Some("next"));
}

#[test]
fn offsets_point_back_at_the_source_for_highlighting() {
    let text = "the connections";
    let tokens = Tokenizer::for_locale("en").tokenize(text);
    let second = &tokens[1];
    assert_eq!(
        &text[second.start as usize..second.end as usize],
        "connections"
    );
}

#[test]
fn the_query_is_tokenized_by_the_same_function_as_the_index() {
    // Parity (§12.2): a query term must arrive at the term dictionary in the
    // form the writer put there.
    let indexed = terms("en", "Managing connections");
    let queried = terms("en", "connections");
    assert!(indexed.contains(&queried[0]));
}
