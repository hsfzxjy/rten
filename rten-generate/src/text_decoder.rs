//! Iterator adapters to decode token IDs into text using `rten-text`.

use rten_text::tokenizers::{Tokenizer, TokenizerError};

use crate::generator::{GeneratorError, GeneratorItem};

/// Wraps a [`Generator`](crate::Generator) to decode the output token IDs from
/// the model into text using a [`Tokenizer`].
///
/// This is normally created by calling [`decode`](crate::GeneratorUtils::decode)
/// on a `Generator`.
pub struct TextDecoder<'a, G: Iterator<Item = GeneratorItem>> {
    generator: G,
    tokenizer: &'a Tokenizer,
}

impl<'a, G> TextDecoder<'a, G>
where
    G: Iterator<Item = GeneratorItem>,
{
    /// Wrap a token generator and decode its outputs using `tokenizer`.
    pub fn wrap(generator: G, tokenizer: &'a Tokenizer) -> TextDecoder<'a, G> {
        TextDecoder {
            generator,
            tokenizer,
        }
    }
}

impl<'a, G: Iterator<Item = GeneratorItem>> Iterator for TextDecoder<'a, G> {
    /// The decoded string, or the error that occurred during generation.
    type Item = Result<String, GeneratorError>;

    /// Run the model repeatedly until it generates a sequence of tokens which
    /// can be decoded into a valid UTF-8 sequence.
    ///
    /// Returns `Some(Ok(text))` if successful, `Some(Err(error))` if an error
    /// occurs during generation or `None` if the end of output has been
    /// reached.
    fn next(&mut self) -> Option<Self::Item> {
        // Buffer that holds model output tokens until it forms a valid UTF-8
        // sequence.
        let mut token_buf = Vec::new();

        for token in self.generator.by_ref() {
            let token = match token {
                Ok(tok) => tok,
                Err(err) => return Some(Err(err)),
            };

            token_buf.push(token);

            let text = self.tokenizer.encoder().decode(&token_buf);
            match text {
                Ok(text) => return Some(Ok(text)),
                Err(TokenizerError::InvalidUtf8) => {
                    // If the current token sequence doesn't correspond to a
                    // complete UTF-8 sequence, add more tokens until it does.
                    continue;
                }
                Err(err) => {
                    return Some(Err(GeneratorError::DecodeError(err)));
                }
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use rten_text::tokenizers::patterns::GPT2;
    use rten_text::tokenizers::{Bpe, TokenId, Tokenizer, WordPiece};

    use crate::{GeneratorError, GeneratorUtils};

    /// Create a simple WordPiece tokenizer. This is essentially just a lookup
    /// from token ID to string.
    fn create_tokenizer() -> Tokenizer {
        let vocab: HashMap<String, TokenId> = [("one", 1), ("two", 2), ("three", 3)]
            .into_iter()
            .map(|(s, id)| (s.to_string(), id))
            .collect();
        let encoder = WordPiece::from_vocab(vocab, Default::default());
        Tokenizer::new(encoder, Default::default())
    }

    /// Create a BPE tokenizer with an empty vocab. This can encode and decode
    /// arbitrary Unicode characters, by using one token per UTF-8 byte.
    fn create_bpe_tokenizer() -> Tokenizer {
        let encoder = Bpe::new(&[], GPT2, None, Default::default()).unwrap();
        Tokenizer::new(encoder, Default::default())
    }

    #[test]
    fn test_decode() {
        let tokenizer = create_tokenizer();
        let generator = [1, 2, 3].into_iter().map(Ok);
        let tokens: Vec<_> = generator
            .decode(&tokenizer)
            .map(|tok| tok.map_err(|e| e.to_string()))
            .collect();
        assert_eq!(tokens, ["one", "two", "three"].map(|s| Ok(s.to_string())));
    }

    #[test]
    fn test_decode_partial_utf8() {
        let tokenizer = create_bpe_tokenizer();

        // Encode a character which will require multiple token IDs. This means
        // the text decoder will need to loop until accumulated tokens decode
        // to a valid UTF-8 sequence.
        let token_ids = tokenizer.encoder().encode("😊").unwrap();
        assert!(token_ids.len() > 1);
        let generator = token_ids.into_iter().map(|tok_id| Ok(tok_id as u32));

        let tokens: Vec<_> = generator
            .decode(&tokenizer)
            .map(|tok| tok.map_err(|e| e.to_string()))
            .collect();

        assert_eq!(tokens, ["😊"].map(|s| Ok(s.to_string())));
    }

    #[test]
    fn test_generate_error() {
        let tokenizer = create_tokenizer();
        let generator = [
            Ok(1),
            Err(GeneratorError::GenerateError("oh no".to_string().into())),
            Ok(3),
        ]
        .into_iter();

        let tokens: Vec<_> = generator
            .decode(&tokenizer)
            .map(|tok| tok.map_err(|e| e.to_string()))
            .collect();

        assert_eq!(
            tokens,
            [
                Ok("one".to_string()),
                Err("generation error: oh no".to_string()),
                Ok("three".to_string())
            ]
        );
    }

    #[test]
    fn test_decode_error() {
        let tokenizer = create_tokenizer();
        let generator = [1, 5, 3].into_iter().map(Ok);

        let tokens: Vec<_> = generator
            .decode(&tokenizer)
            .map(|tok| tok.map_err(|e| e.to_string()))
            .collect();

        assert_eq!(
            tokens,
            [
                Ok("one".to_string()),
                Err("decode error: unknown token id 5".to_string()),
                Ok("three".to_string())
            ]
        );
    }
}
