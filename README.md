# Sissy

Limpador assistido, offline, em Rust + egui. A interface e o mecanismo rodam no mesmo processo; uma thread de análise mantém a janela responsiva. Não há servidor web, Python, conta, telemetria ou serviço remoto. O scanner é somente leitura; a única mutação dos arquivos analisados é um pedido explícito ao Finder para mover a seleção ao Lixo.

## Desenvolver e testar

Instale Rust estável (1.88 ou superior; o lockfile foi resolvido com 1.99.1) pelo rustup. No Mac, instale também `xcode-select --install`.

```sh
cd sissy
cargo test --locked
cargo run --locked
```

Internet é necessária para obter Rust e dependências durante o build, não para executar o app. `Cargo.lock` fixa dependências; depois de baixar as dependências, use `cargo build --locked --offline`. No Linux, a interface requer bibliotecas de desenvolvimento X11/Wayland/OpenGL e portal de seleção de arquivos; Linux serve para desenvolvimento, não é uma distribuição suportada do produto.

## Gerar o aplicativo e o DMG no Mac

```sh
./scripts/package-macos.sh universal
# Alternativas: ./scripts/package-macos.sh arm64
#               ./scripts/package-macos.sh x86_64
```

Saída em `dist/<modo>/Sissy.app` e `Sissy-<modo>.dmg`. O script usa rustup, cargo, lipo, Quick Look, sips, iconutil, codesign e hdiutil. Usa o SVG original em `assets/sissy.svg`, renderizado a 1024 px para gerar todos os tamanhos de ICNS. Verifique visualmente o resultado de Quick Look no Mac. Abra o DMG e arraste Sissy para Aplicativos.

Alvo mínimo declarado: macOS 11 (Big Sur). Universal contém arm64 + x86_64; arm64 só Apple Silicon; x86_64 para Intel ou Rosetta em Apple Silicon. A compilação macOS, integração Finder, permissões, ícone e execução nesses sistemas ainda precisam de validação em hardware macOS. Nenhum `.app` ou `.dmg` macOS foi gerado neste ambiente Linux.

O script aplica apenas assinatura ad-hoc para execução local. **Não há assinatura Developer ID nem notarização.** Para distribuição, é necessário Apple Developer Program, certificado Developer ID Application com chave privada no Keychain, e perfil de credenciais do notarytool (Apple ID + senha específica e Team ID, ou chave API App Store Connect). Assine o bundle com `codesign --force --options runtime --timestamp --sign 'Developer ID Application: …' dist/universal/Sissy.app`; teste a automação do Finder sob hardened runtime e as permissões apropriadas antes de distribuir. Recrie o DMG com o app assinado, submeta com `xcrun notarytool submit <dmg> --keychain-profile <perfil> --wait`, aplique `xcrun stapler staple <dmg>` e valide com `spctl` e `stapler validate`. Não embuta credenciais no repositório.

## Usar com cuidado

A lista inicial contém Downloads, Documents e Desktop existentes. Nada é analisado até clicar em Analisar. Adicione ou remova pastas antes de começar. A versão atual só permite árvores dentro de HOME; pastas externas e locais protegidos são ignorados. Não oferece varredura avançada do sistema nem segue links simbólicos. Para caches, escolha explicitamente `~/Library/Caches`. Não inclui `/tmp` nem presume que caches sejam descartáveis. Diretórios exigindo autorização não são contornados; a visualização avançada mostra erros. Conceda acesso nas configurações de Privacidade do macOS se desejar, depois repita a análise. Para mover ao Lixo, o macOS pode solicitar autorização de Automação para controlar o Finder.

Os critérios são pistas: tamanho lógico configurável, idade desde a modificação de Downloads, cache conhecido, extensão de vídeo/instalador/arquivo e SHA-256 para duplicados do mesmo tamanho. Não são provas de inutilidade. Em cada grupo duplicado, a primeira cópia encontrada é indicada como preservada; as demais são candidatas. A ordem de descoberta não significa preferência. A cópia preservada nunca pode integrar o mesmo lote de remoção. Arquivos pequenos ainda podem ser duplicados. Datas aparecem como segundos Unix, sem conversão local nesta versão.

Selecione cada arquivo individualmente. Revise contagem e bytes, abra ou revele no Finder, depois confirme explicitamente o envio ao Lixo. Nenhuma exclusão permanente é oferecida. O espaço só pode ser recuperado depois de esvaziar o Lixo manualmente, e pode diferir do tamanho lógico (APFS clones, arquivos esparsos e hard links). Cancelar interrompe a travessia/leitura por blocos; os resultados são marcados parciais. Chamadas de sistema bloqueadas podem atrasar o cancelamento.

Resultados das operações são registrados na sessão, por item numerado; falhas não interrompem o restante do lote. O app solicita confirmação separada para persistir/exportar um log, explicando seu conteúdo. Por padrão só contagens e estados; caminhos da análise apenas com opt-in explícito. Nenhum log é criado automaticamente. A exportação não é criptografada. Salve em pasta local se não quiser sincronização por software externo (iCloud, por exemplo).

## Verificação de segurança e limitações

- Leia `scan` em `src/lib.rs`: somente enumera, lê metadados e conteúdo. `act` retorna sem chamar a operação se `confirmed=false`; os testes verificam isso. No app, só o botão de confirmação chama `act(..., true, trash)`.
- Testes cobrem conteúdo igual/diferente com mesmo tamanho, raízes sobrepostas, exclusão de symlinks e sistema, cancelamento prévio, arquivo alterado, confirmação e falha parcial injetada. Testes de Finder real não foram executados no Linux. Use uma pasta descartável no Mac para verificar confirmar/cancelar, Lixo, restauração, permissões recusadas e falhas entre itens.
- Antes de mover, revalida caminho, tamanho, data e conteúdo da cópia preservada. Isso reduz, mas não elimina, corridas entre verificação e ação do Finder. Não use em árvores modificadas simultaneamente ou sob controle de terceiros. SHA-256 não é comparação byte a byte; colisões são teoricamente possíveis. Hard links não são deduplicados por inode. Arquivos podem mudar durante a análise; resultados exigem revisão humana.
- Não há cliente HTTP ou endpoints no código do app. Audite `cargo tree` e o código; execute com a rede desligada e monitore o processo no macOS (firewall/instrumentação de rede) para verificar ausência de tráfego. Dependências gráficas e ferramentas do sistema também exigem auditoria para uma garantia formal. Abrir arquivos aciona seus aplicativos associados, que podem ter seu próprio comportamento de rede.
- A interface simples e a avançada compartilham os resultados; a avançada acrescenta os avisos. Não há pré-visualização de conteúdo, seleção de grupo, preferências persistentes nem garantias de acessibilidade com VoiceOver. Revise contraste, teclado e leitor de tela no Mac antes de distribuição ampla. Comic Sans é carregada quando instalada; caso contrário usa fonte proporcional incluída.

## Estrutura

`src/lib.rs`: descoberta, classificação, hash, revalidação e ações; `src/main.rs`: interface e confirmação; `tests/core.rs`: testes de integração; `scripts/package-macos.sh`: build e embalagem; `assets/sissy.svg`: ícone vetorial original.

## Validação desta entrega

No ambiente Linux: `cargo test --locked` compilou a biblioteca e o executável e executou **5 testes de integração, todos aprovados**. `bash -n scripts/package-macos.sh` passou. Como HOME da máquina é somente leitura, os testes foram rodados com `HOME=/workspace/sissy-test-home` (diretório descartável criado previamente); isso também exercita a restrição de escopo ao usuário sem acessar arquivos reais. A tentativa de `cargo run --locked` compilou, mas a janela não abriu: o executor não possui DISPLAY/Wayland configurado. Portanto, aparência e interação não foram verificadas visualmente nesta entrega. Build universal, DMG, Lixo via Finder e assinatura ad-hoc estão implementados em código/scripts, mas ainda não foram executados no macOS. Toolchain fixado em `rust-toolchain.toml`; nenhum binário macOS é incluído.

## Build Linux e captura da interface (atualização)

Compilação e execução gráfica verificadas em Debian 13 x86_64, com Xvfb e renderização Mesa por software. A análise foi acionada pela interface com dois arquivos locais de exemplo: 2 examinados, 1 candidato duplicado, 0 avisos. Os 5 testes passaram novamente. As capturas reais estão em `artifacts/sissy-linux-inicio.png` e `artifacts/sissy-linux-resultados.png`; não são mockups. Esta validação substitui a limitação anterior de ausência de display: foi instalado um Xvfb local para executar a janela.

Para reproduzir em Linux x86_64 com Rust e sessão gráfica X11/Wayland:

```sh
./scripts/build-linux.sh
./dist/linux-x86_64/sissy
```

O pacote `dist/Sissy-linux-x86_64-debug.tar.gz` contém build de desenvolvimento, sem otimização de release. Foi compilado com glibc 2.41 (Debian 13); não se garante execução em distribuições antigas. O backend gráfico carrega bibliotecas X11/Wayland e OpenGL dinamicamente; diálogos precisam de um portal de desktop. Na sessão virtual o teste cobriu a janela e a análise, não o seletor de pastas ou exportação de log. Finder, abertura e Lixo ainda não têm implementação Linux: o backend de Lixo retorna erro sem mover nada. Não confundir build Linux com suporte completo às ações do macOS.

### Ícone fornecido pelo usuário

`assets/SISSY_ICON_MAC.icon/` preserva a composição original do Icon Composer e seu PNG, sem alterar o desenho. O PNG agora é embutido no executável como ícone da janela e mostrado no cabeçalho. A composição em camadas, iluminação e fundo definidos no Icon Composer não são reproduzidos pelo egui. O SVG original anterior continua disponível. O script macOS ainda usa o SVG para ICNS; integrar a composição nativa `.icon` exige validação com ferramentas Apple em um Mac.

## Interface revisada

A interface agora segue a referência visual fornecida: cabeçalho verde, marca e mascote, navegação lateral (Iniciante, Avançado, Resultados, Relatórios e Dicas), painéis com bordas e sombras sólidas, botões maiores e detalhes de cada resultado expansíveis. As variantes regular e negrito de Comic Sans enviadas pelo usuário estão em `assets/fonts` e são embutidas no executável; não exigem instalação externa. O contador de progresso informa arquivos examinados, sem porcentagem fictícia. Não há resultados de demonstração embutidos: as capturas usam uma pasta descartável com dois arquivos reais iguais.

As novas capturas estão em `artifacts/sissy-interface-nova.png` e `artifacts/sissy-interface-resultados.png`. No Linux, ações de Finder e Lixo ficam explicitamente desabilitadas. A aparência foi verificada no display virtual Linux; a integração macOS ainda requer um Mac. Os testes do núcleo continuam separados da interface.

## Distribuição pelo GitHub Actions

O workflow `.github/workflows/macos.yml` roda em `macos-14` após alterações de código na branch `main` ou acionamento manual. Ele executa testes, gera o bundle universal e o DMG, valida plist/arquiteturas/assinatura ad-hoc/integridade do DMG, arquiva o `.app` em ZIP e publica uma pré-release com os dois downloads e SHA-256. Consulte Actions e Releases para confirmar o resultado de cada execução. A inclusão do workflow não comprova que um build já terminou: permissões, disponibilidade do runner e possíveis erros aparecem na execução do GitHub. A assinatura é apenas ad-hoc, sem Developer ID ou notarização. O ICNS agora é gerado com o PNG fornecido pelo usuário; a composição nativa Icon Composer continua preservada como fonte.

### Atualização do Icon Composer

A composição atualizada em `assets/SISSY_ICON_MAC.icon` contém a camada colorida e `sissymono2.png` para aparência tingida. O empacotamento agora exige Xcode completo 26+ e chama `actool` para gerar os recursos nativos, com `CFBundleIconName=SISSY_ICON_MAC`. O workflow usa macOS 15 e seleciona o Xcode estável mais recente disponível. O ICNS continua como fallback para macOS antigos. Esta integração precisa ser validada no runner macOS; não foi executada no Linux.

O PNG interno `Cheerful Lime Snake Mascot.png` não mudou nesta atualização e continua usado pela interface e pelo fallback ICNS. A versão PNG com fundo verde mostrada na conversa ainda não está disponível como arquivo no checkout; sua substituição nas outras plataformas está pendente do envio do arquivo PNG original.
