#![feature(nll)]

extern crate pest;
#[macro_use]
extern crate pest_derive;
#[cfg(test)]
#[macro_use]
extern crate pretty_assertions;

mod grammar;
pub mod tokens;

use grammar::*;
use pest::Parser;
use tokens::*;

pub struct Posticle<'t> {
    transformer: &'t TokenTransformer,
}

impl<'t> Posticle<'t> {
    pub fn new() -> Self {
        Self {
            transformer: &DefaultTransformer,
        }
    }

    pub fn from_transformer(transformer: &'t TokenTransformer) -> Self {
        Self { transformer }
    }

    /// Given `text`, extract all [Entities](Token)
    pub fn parse(&self, text: &str) -> Vec<Token> {
        let mut tokens: Vec<Token> = Vec::new();

        if let Ok(pairs) = Grammar::parse(Rule::document, text) {
            for pair in pairs {
                tokens.append(&mut Token::from_parse_pair(pair));
            }
        }

        self.apply_transformer(tokens)
    }

    /// The parser has a tendency to produce rows of text tokens, combine any text token that follows another text token into a new text token.
    fn normalize_text_tokens(&self, input: Vec<Token>) -> Vec<Token> {
        let mut output = Vec::new();
        let mut replacement = String::new();

        for token in input {
            match token {
                Token::Text(Text(text)) => {
                    replacement.push_str(&text);
                },
                _ => {
                    if replacement.len() > 0 {
                        output.push(Token::Text(Text(replacement)));
                        replacement = String::new();
                    }

                    output.push(token);
                },
            }
        }

        if replacement.len() > 0 {
            output.push(Token::Text(Text(replacement)));
        }

        output
    }

    fn apply_transformer(&self, input: Vec<Token>) -> Vec<Token> {
        let transformer = self.transformer;
        let mut output = Vec::new();

        for token in input {
            output.append(&mut transformer.transform(token));
        }

        self.normalize_text_tokens(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_nothing() {
        let posticle = Posticle::new();

        assert_eq!(
            posticle.parse("a string without at signs"),
            vec![Token::Text(Text("a string without at signs".to_string()))]
        );
    }

    #[test]
    fn extracts_mentions() {
        let posticle = Posticle::new();

        assert_eq!(
            posticle.parse("@mention"),
            vec![Token::Mention(Mention("mention".to_string(), None))]
        );
        assert_eq!(
            posticle.parse("@mention@domain.place"),
            vec![Token::Mention(Mention(
                "mention".to_string(),
                Some("domain.place".to_string())
            ))]
        );
        assert_eq!(
            posticle.parse("@Mention@Domain.Place"),
            vec![Token::Mention(Mention(
                "Mention".to_string(),
                Some("Domain.Place".to_string())
            ))]
        );
    }

    #[test]
    fn extracts_mentions_in_punctuation() {
        let posticle = Posticle::new();

        assert_eq!(
            posticle.parse("(@mention)"),
            vec![
                Token::Text(Text("(".to_string())),
                Token::Mention(Mention("mention".to_string(), None)),
                Token::Text(Text(")".to_string()))
            ]
        );
    }

    #[test]
    fn ignores_invalid_mentions() {
        let posticle = Posticle::new();
        let mentions = vec![
            "some text @ yo",
            "@@yuser@domain",
            "@xuser@@domain",
            "@@not",
            "@not@",
            "@not@@not",
            "@not@not@not",
        ];

        for mention in mentions {
            assert_eq!(
                posticle.parse(mention),
                vec![Token::Text(Text(mention.to_string()))],
                "ignores_invalid_mentions failed on {}",
                mention
            );
        }
    }

    #[test]
    fn extracts_hashtags() {
        let posticle = Posticle::new();
        let hashtags = vec!["#hashtag", "#HASHTAG", "#1000followers", "#文字化け"];

        for hashtag in hashtags {
            assert_eq!(
                posticle.parse(hashtag),
                vec![Token::Hashtag(Hashtag(hashtag[1..].to_string()))],
                "extracts_hashtags failed on {}",
                hashtag
            );
        }
    }

    #[test]
    fn extracts_hashtags_in_punctuation() {
        let posticle = Posticle::new();
        let hashtags = vec!["#hashtag", "#HASHTAG", "#1000followers", "#文字化け"];

        for hashtag in hashtags {
            assert_eq!(
                posticle.parse(&format!("({})", hashtag)),
                vec![
                    Token::Text(Text("(".to_string())),
                    Token::Hashtag(Hashtag(hashtag[1..].to_string())),
                    Token::Text(Text(")".to_string()))
                ],
                "extracts_hashtags_in_punctuation failed on {}",
                hashtag
            );
        }
    }

    #[test]
    fn ignores_invalid_hashtags() {
        let posticle = Posticle::new();
        let hashtags = vec![
            "some text # yo",
            "##not",
            "#not#",
            "#not##not",
            "#not#not#not",
        ];

        for hashtag in hashtags {
            assert_eq!(
                posticle.parse(hashtag),
                vec![Token::Text(Text(hashtag.to_string()))],
                "ignores_invalid_hashtags failed on {}",
                hashtag
            );
        }
    }

    #[test]
    fn extracts_links() {
        let posticle = Posticle::new();
        let links = vec![
            "http://example.com",
            "http://example.com/path/to/resource?search=foo&lang=en",
            "http://example.com/#!/heck",
            "HTTP://www.ExaMPLE.COM/index.html",
            "http://example.com:8080/",
            "http://test_underscore.example.com",
            "http://☃.net/",
            "http://example.com/Русские_слова",
            "http://example.com/الكلمات_العربية",
            "http://sports.yahoo.com/nfl/news;_ylt=Aom0;ylu=XyZ?slug=ap-superbowlnotebook",
            "http://example.com?foo=$bar.;baz?BAZ&c=d-#top/?stories",
            "http://www.youtube.com/watch?v=g8X0nJHrJ9g&list=PLLLYkE3G1HEAUsnZ-vfTeQ_ZO37DhHhOY-",
            "http://www.example.com/",
        ];

        for link in links {
            assert_eq!(
                posticle.parse(link),
                vec![Token::Link(Link(link[7..].to_string(), link.to_string()))],
                "extracts_links failed on {}",
                link
            );
        }
    }

    #[test]
    fn extracts_links_in_punctuation() {
        let posticle = Posticle::new();
        let links = vec![
            "http://example.com",
            "http://example.com/path/to/resource?search=foo&lang=en",
            "http://example.com/#!/heck",
            "HTTP://www.ExaMPLE.COM/index.html",
            "http://example.com:8080/",
            "http://test_underscore.example.com",
            "http://☃.net/",
            "http://example.com/Русские_слова",
            "http://example.com/الكلمات_العربية",
            "http://sports.yahoo.com/nfl/news;_ylt=Aom0;ylu=XyZ?slug=ap-superbowlnotebook",
            "http://example.com?foo=$bar.;baz?BAZ&c=d-#top/?stories",
            "http://www.youtube.com/watch?v=g8X0nJHrJ9g&list=PLLLYkE3G1HEAUsnZ-vfTeQ_ZO37DhHhOY-",
            "http://www.example.com/",
        ];

        for link in links {
            assert_eq!(
                posticle.parse(&format!("({})", link)),
                vec![
                    Token::Text(Text("(".to_string())),
                    Token::Link(Link(link[7..].to_string(), link.to_string())),
                    Token::Text(Text(")".to_string()))
                ],
                "extracts_links_in_punctuation failed on {}",
                link
            );
        }
    }

    #[test]
    fn ignores_invalid_links() {
        let posticle = Posticle::new();
        let links = vec!["x- text http:// yo", "x-:thing", "x-://thing/else yo"];

        for link in links {
            assert_eq!(
                posticle.parse(link),
                vec![Token::Text(Text(link.to_string()))],
                "ignores_invalid_links failed on {}",
                link
            );
        }
    }

    #[test]
    fn extracts_all() {
        let posticle = Posticle::new();

        assert_eq!(
            posticle.parse("text #hashtag https://example.com @mention text"),
            vec![
                Token::Text(Text("text ".to_string())),
                Token::Hashtag(Hashtag("hashtag".to_string())),
                Token::Text(Text(" ".to_string())),
                Token::Link(Link(
                    "example.com".to_string(),
                    "https://example.com".to_string()
                )),
                Token::Text(Text(" ".to_string())),
                Token::Mention(Mention("mention".to_string(), None)),
                Token::Text(Text(" text".to_string())),
            ]
        );
    }
}
