# Bora deixar o Sissy mais humano?

O protótipo teve ajuda de IA. Agora queremos mais gente criando, questionando, revisando e dando personalidade ao projeto. Não precisa ser especialista nem começar com uma mudança grande.

## Tem espaço para você

- **Código:** Rust, egui, integração com macOS, desempenho e tratamento de erros.
- **Design e acessibilidade:** proporções, contraste, navegação por teclado, leitores de tela e mensagens mais claras.
- **Tradução:** revisar português, inglês, espanhol e japonês, especialmente os textos que soam pouco naturais.
- **Testes:** outros Macs, versões do macOS, discos externos, diferentes tamanhos de janela e permissões.
- **Arte e documentação:** a cobrinha, ícones, guias, exemplos e ideias para a identidade do Sissy.

Pode fazer um fork, modificar o código e experimentar outro caminho. Se quiser trazer sua mudança para o projeto, abra um pull request contando o problema, a solução e como conferiu o resultado. Para algo maior, uma issue antes ajuda a combinar o caminho.

Fontes, ícones e outros recursos de terceiros conservam suas próprias licenças e condições; este convite não muda os direitos desses materiais.

## Primeiro contato com o código

1. Instale a ferramenta Rust indicada em `rust-toolchain.toml` e as ferramentas de desenvolvimento Apple.
2. Execute `cargo test --locked` e `cargo run --locked`.
3. Para traduções, edite `src/i18n.tsv`, execute `python3 scripts/generate-i18n.py` e depois `cargo fmt --all`.
4. Antes de enviar, confira `cargo fmt --all --check` e os testes relevantes à mudança.

O [README](README.md) descreve o empacotamento, as limitações e as verificações feitas no Mac. Para testar a análise, prefira uma pasta descartável com cópias de arquivos. Não inclua arquivos pessoais, credenciais, relatórios com caminhos sensíveis nem caches de compilação no pull request.

## Algumas escolhas merecem cuidado

A análise é somente leitura. O envio ao Lixo exige revisão e confirmação; não há exclusão permanente como alternativa. Links simbólicos e locais protegidos ficam de fora. Mudanças nessas partes precisam de atenção e testes que expliquem seu efeito.

Não chame um arquivo de inútil só porque ele é grande ou antigo. O aplicativo oferece pistas; a pessoa decide o que fica.

## IA pode ajudar. Pessoas também precisam entender

Ferramentas de IA são bem-vindas no processo. Revise o que elas produzem, entenda a alteração e conte o que conseguiu testar e o que ainda não conseguiu. Não precisa fingir certeza nem esconder uma dúvida.

Uma boa contribuição pode ser uma correção de duas linhas, uma frase mais acolhedora ou um problema bem explicado. Queremos um aplicativo com cuidado humano em cada detalhe.

**© 2026 STRIKER Code Team** · [github.com/leonardobluestriker](https://github.com/leonardobluestriker)
