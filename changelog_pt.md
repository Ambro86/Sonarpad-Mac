Registo de alterações

Versão 0.4.0 - 3 de setembro de 2026

Audiodescrição com IA — nova função principal

- Foi adicionada a opção «Criar audiodescrição com IA» diretamente ao menu Ferramentas. O Sonarpad analisa o áudio para encontrar espaços sem diálogo, gera as descrições com Gemini e utiliza os motores de voz já disponíveis, evitando falar por cima dos diálogos.

- Foi melhorada a sincronização entre o que acontece no vídeo e as descrições, com verificações automáticas dos tempos gerados pelo Gemini.

- «Ativar pausas prolongadas» está desativado por predefinição. Pode ser ativado em conteúdos com muito diálogo ou pouco espaço disponível para permitir descrições mais longas.

- O Sonarpad pode tentar reconhecer personagens e utilizar os seus nomes. Os catálogos de personagens podem ser mantidos entre episódios de uma série para melhorar a continuidade.

- É possível guardar o projeto, editar posteriormente as descrições e voltar a exportar sem ter de gerar tudo novamente com o Gemini.

- Se o processo for interrompido, o Sonarpad conserva o progresso e permite continuar a audiodescrição. Se a quota do Gemini se esgotar, é possível esperar, mudar de modelo ou interromper sem perder o trabalho já concluído.

- A janela permite escolher idioma, nível de detalhe, modelo Gemini, motor de voz e voz, e memoriza as preferências utilizadas. O módulo está disponível nos idiomas suportados pelo Sonarpad para Mac.

- Durante a geração, a interface mostra o progresso, o estado atual e Cancelar; no final, o MP3 pode ser aberto diretamente no leitor interno.

- Melhorada a compatibilidade com vídeos MKV: o Sonarpad lida de forma mais fiável com marcas de tempo irregulares ou ausentes e, sempre que possível, ignora pacotes corrompidos sem interromper a audiodescrição.

- Corrigido um problema que podia fazer falhar a exportação final para MP3 em vídeos com áudio multicanal, como Dolby 5.1. O Sonarpad converte automaticamente o áudio multicanal para estéreo quando necessário para a codificação MP3.

- Quando um vídeo contém várias faixas de áudio, o Sonarpad pergunta qual faixa deve ser utilizada antes do processamento. A caixa de combinação acessível pode ser alterada com as setas; OK inicia a audiodescrição com a faixa selecionada, enquanto Cancelar fecha a janela e devolve o foco ao editor do Sonarpad.

- Adicionada a caixa «Mostrar chave API» junto à chave Gemini. A chave permanece oculta por predefinição e só é mostrada temporariamente enquanto a caixa está ativa; ao reabrir a janela, volta sempre a ficar oculta.

YouTube e streaming

- A experiência do YouTube foi significativamente melhorada, tornando a pesquisa e a navegação mais rápidas e restabelecendo o seu funcionamento correto.

- As opções de qualidade de vídeo estão agora traduzidas: em vez do valor técnico «best», o Sonarpad mostra uma etiqueta clara no idioma da interface.

- O Sonarpad memoriza o último formato escolhido em Guardar mídia. Por exemplo, se MP4 for selecionado, MP4 continuará pré-selecionado na próxima vez que a janela for aberta.


Agradecimentos

- Um agradecimento especial a Leonardo Graziano e Tiziano Ferraro, que testaram em profundidade a função de Audiodescrição com IA e o Sonarpad em geral, contribuindo de forma valiosa para a sua melhoria.

- Um grande agradecimento também ao grupo Tecnologia Accessibile pelo apoio, testes e sugestões.

Versão 0.3.1 - 16 de julho de 2026

- Foi corrigido um problema que impedia o Sonarpad de iniciar quando o menu Rádio continha favoritos, devido a identificadores de menu inválidos no wxWidgets.

- O Sonarpad está agora também disponível em francês, espanhol, português, checo e polaco, além de italiano e inglês.

- Foi adicionada uma definição separada para o Idioma das notícias. Esta definição é independente do idioma da interface e permite que o Sonarpad utilize fontes e serviços adaptados ao idioma selecionado.

- Foi adicionada a funcionalidade Meteorologia, que permite pesquisar uma cidade e consultar as condições atuais, a temperatura, a precipitação, o vento e a humidade, bem como as previsões para hoje, amanhã ou outro dia.

- Foi adicionada a secção Filmes no cinema, com os filmes atualmente em exibição, as próximas estreias, os resumos, as datas de estreia e, quando disponíveis, ligações para os trailers.

- Foi adicionado um calendário acessível ao menu Ferramentas. É possível selecionar qualquer data, consultar feriados, o santo e a frase do dia, criar lembretes e adicionar compromissos diretamente ao Calendário do macOS.

- Foi adicionada a funcionalidade Pesquisa de percursos, que permite calcular trajetos a pé, de bicicleta, de automóvel ou acessíveis a cadeiras de rodas. É possível escolher o percurso mais rápido ou o mais curto e consultar a distância, a duração estimada e as indicações detalhadas.

- Foi adicionada a funcionalidade Converter multimédia, que permite converter ficheiros de áudio e vídeo para vários formatos, incluindo MP3, M4A, M4B, MP4, AVI, MOV, Opus, OGG, FLAC, WAV e AIFF. Também é possível criar um vídeo a partir de um ficheiro de áudio e de uma imagem.

- Foi adicionado o Dicionário de voz. É possível definir palavras ou expressões que o sintetizador de voz deve substituir durante a leitura, permitindo corrigir pronúncias, abreviaturas e nomes específicos.

- A secção Artigos foi ampliada com os comandos Artigos recentes e Partilhar, permitindo regressar rapidamente aos últimos conteúdos lidos e partilhar artigos através dos serviços disponíveis no macOS.

- Foram adicionadas ao menu Artigos as opções Adicionar uma fonte de notícias à comunidade Sonarpad e Fontes de notícias da comunidade Sonarpad. É possível enviar um feed RSS ou um site de notícias e importar fontes partilhadas por outros utilizadores. As fontes são adicionadas e apresentadas de acordo com o Idioma das notícias selecionado.

- Foi melhorada a gestão das fontes de notícias. Alterar o Idioma das notícias passa a carregar as fontes predefinidas adequadas sem remover as fontes adicionadas pessoalmente pelo utilizador.

- A pesquisa de rádios foi ampliada com a navegação por idioma, país e cidade, incluindo os nomes completos e localizados dos países.

- Foi adicionada a possibilidade de enviar uma estação de rádio para a comunidade Sonarpad, indicando o nome, o endereço de transmissão, o idioma e o género.

- Foram adicionadas a gravação de rádio e a gravação de rádio programada. Estas ações estão disponíveis nos resultados da pesquisa e nos favoritos, e as gravações são guardadas diretamente como ficheiros MP3. Depois de abrir uma estação de rádio, também é possível iniciar a gravação premindo a letra R.

- Foi adicionada ao menu Ficheiro uma lista dos documentos de texto abertos recentemente, facilitando a sua reabertura.

- Foi adicionado o modo Só de leitura, útil para consultar um documento sem o modificar acidentalmente.

- Foi adicionado o Conteúdo do livro para ficheiros EPUB que incluem um índice. É possível selecionar um capítulo e ir diretamente para ele.

- Foi adicionada a opção de escolher entre as vozes Microsoft de alta qualidade e as vozes de sistema do macOS.

- Foi adicionada uma opção para ignorar durante a leitura as pausas causadas por linhas vazias.

- Foi adicionada uma definição para escolher quantos segundos avançar ou recuar durante a reprodução multimédia.

- Foi melhorada a acessibilidade das janelas, dos menus e dos controlos, com uma gestão mais coerente do foco, das teclas Enter e Escape e dos atalhos de teclado.

- Foi melhorada a localização das mensagens, dos botões e das janelas de confirmação em todos os idiomas suportados.

- Foi corrigido um problema que impedia a apresentação dos ficheiros multimédia na janela de vídeo do leitor.

- Foram corrigidos numerosos problemas que afetavam a estabilidade, a reprodução multimédia, as gravações de rádio programadas, a gestão das fontes e a compilação no macOS.

- Um agradecimento especial a Leonardo Graziano, Luca Maianti e ao grupo italiano Tecnologia Accessibile pelo apoio contínuo e pelos testes beta constantes.

Versão 0.2.9 - 1 de maio de 2026
- As funcionalidades do YouTube foram estendidas também aos Macs Intel e Catalina.
- A pesquisa no YouTube ficou muito mais rápida.
- A gestão dos resultados do YouTube foi melhorada, colocando canais e listas de reprodução no início.
- Foi adicionada a possibilidade de adicionar e remover canais e listas de reprodução dos favoritos.
- Foi adicionado o botão Pré-visualização da voz nas opções.
- Foi adicionado o botão Selecionar tudo ao remover fontes.
- Foi adicionada uma barra de progresso para a pesquisa na Wikipédia.
- Foi adicionado o canal de TV Videolina.
- As entradas de menu das funcionalidades adicionais foram movidas para Ferramentas, para alinhar o Sonarpad com a versão para Windows.
- Foi corrigido o comportamento em que, por vezes, os programas atualmente no ar não eram mostrados na TV.
- Foram adicionados numerosos canais de TV, organizando a janela em categorias para facilitar a consulta. Também foi adicionado um campo de pesquisa que mostra os resultados da TV pretendida.

Versão 0.2.8 - 29 de abril de 2026
- Foi adicionado o menu Ferramentas com duas novas entradas: Pesquisar e importar da Wikipédia e Reproduzir áudio em streaming.
- Pesquisar e importar da Wikipédia permite pesquisar e importar artigos, lê-los e guardá-los como audiolivros.
- Reproduzir áudio em streaming permite reproduzir conteúdos em streaming, por exemplo do YouTube.
- Na caixa de pesquisa de streaming é possível escrever qualquer conteúdo: o programa irá pesquisá-lo e também poderá abrir canais e listas de reprodução.
- A pesquisa do YouTube não está ativada nos Macs Intel por motivos de incompatibilidade.
- Agradecimento especial a Leonardo Graziano pelo apoio contínuo.
- Nas rádios foi adicionado um botão para ir diretamente para a página selecionada nos resultados, sem ter de usar sempre Ir para a página seguinte.
- O marcador automático foi estendido também aos ficheiros de texto.
- Foi corrigido um problema pelo qual, por vezes, as audiodescrições não eram guardadas devido a problemas de tempo limite.
- Foi adicionada a possibilidade de definir TVs favoritas.
- Na lista de canais de TV foi adicionada a indicação do programa atualmente no ar.
- Foi inserido um guia TV completo, consultável desde o dia anterior até cinco dias após a data atual.

Versão 0.2.7 - 28 de abril de 2026
- Foi melhorado o suporte para ficheiros com diacríticos e codificações diferentes de UTF-8, incluindo suporte para caracteres chineses e outros idiomas internacionais.
- Foi corrigido o problema em que a vírgula, escrita num campo de texto, abria incorretamente as opções.
- A velocidade de leitura foi melhorada: agora também os artigos longos são lidos mais rapidamente e a pausa após os parágrafos foi removida.
- Foi adicionada a possibilidade de abrir com o Sonarpad ficheiros JPG e formatos semelhantes, para permitir OCR também em artigos enviados como imagens ou fotografias.
- Foi adicionada a possibilidade de definir o Sonarpad como programa predefinido.
- A partir de agora o Sonarpad pode abrir não apenas ficheiros de texto, mas também ficheiros de áudio e vídeo, usando o leitor MPV.
- Foi adicionada nas opções a função de marcador automático: se fechar um ficheiro, um podcast ou qualquer conteúdo multimédia, este será reaberto exatamente da posição em que ficou.
- As rádios já não são abertas no Safari, mas reproduzidas diretamente através do leitor do Sonarpad.
- A partir desta versão a app está assinada e já não requer qualquer autorização do utilizador, tornando a instalação mais simples.
- Foi adicionada uma atualização automática do programa que verifica, descarrega e atualiza automaticamente o Sonarpad.
- Foram inseridos os módulos adicionais RaiPlay, Audiodescrições Rai, RaiPlay Sound e canais TV. Para os utilizar será necessário pedir um código ao autor.
- Para obter o código, siga o procedimento indicado pelo programa e envie o e-mail gerado, certificando-se de que ele está realmente presente no correio enviado. Se o procedimento for executado corretamente, o código será recebido dentro de cerca de um minuto.
- O código deve ser introduzido abrindo as opções com Command + , e deslocando-se com VO + seta direita até ao campo Código Sonarpad para funcionalidades adicionais.
- Nota: se, ao abrir uma funcionalidade adicional, por exemplo RaiPlay, aparecer um erro, isso significa provavelmente que o código não foi copiado integralmente.
- Nos módulos Rai foram adicionadas a pesquisa e a consulta de conteúdos, que são reproduzidos através do leitor do Sonarpad.

Versão 0.2.6
- Foi corrigido um erro de wx/macOS que podia apresentar um erro no arranque e os menus ligados foram estabilizados.
- Foi corrigido o atalho Cmd+, para o menu Opções mesmo quando o foco está no editor ou noutros controlos.
- Ao guardar um audiolivro, o foco é agora colocado corretamente no campo de texto e os nomes de ficheiro com ponto já não são cortados.
- Foi adicionado suporte para OPML do Lire com divisão em pastas: as pastas abrem como submenus e as fontes individuais numa janela dedicada.
- A reordenação das fontes de artigos agora gere o novo sistema de pastas com botões Abrir pasta, Pasta principal, Mover para pasta e Mover para fora das pastas.

Versão 0.2.5
- Novas janelas de gravação personalizadas para texto e audiolivros em macOS.
- Os campos de nome de ficheiro agora aceitam corretamente Cmd+V, Cmd+A e os outros comandos de edição.
- O programa lembra a última pasta e o último formato usados para guardar texto e audiolivros.
- Foi adicionada a gravação de audiolivros também em formato M4A e WAV.
- Foi adicionado o menu Rádio com pesquisa por idioma, adicionar aos favoritos, adicionar manualmente uma estação e editar e reordenar favoritos.
- Foi melhorada a gestão das fontes de artigos inseridas como sites: descoberta do feed a partir da página e correção do feed de comentários.
- O fluxo de lançamento macOS foi atualizado para incluir também o artefacto Catalina.

Versão 0.2.4
- Melhorias importantes no OCR de PDF em macOS com a passagem para pdfium e alternativas mais robustas.
- Foi adicionada a exportação M4B em macOS e aperfeiçoada a gravação de texto.
- Foi melhorada a gestão das fontes de artigos e a proteção da atualização quando uma fonte devolve zero itens.
- Foi otimizada a síntese Edge TTS com divisão em blocos e novas tentativas mais fiáveis.
- Foi adicionada e refinada a pipeline Catalina para compilação e empacotamento macOS.

Versão 0.2.2
- Foi melhorado o carregamento de PDFs em macOS com feedback mais claro e uma janela final explícita.
- Ordenação alfabética das fontes de artigos.
- Reparações no texto PDF e melhorias gerais de localização.

Versão 0.2.1
- Foram estabilizados os atalhos e menus macOS para iniciar, pausar, parar e guardar.
- Foi melhorada a abertura externa dos episódios de podcast em macOS.
- Foi corrigida a persistência das opções em macOS.
- Foram reforçados os fluxos de compilação Intel/macOS e a gestão do Xcode.

Versão 0.2.0
- Primeira versão macOS do Sonarpad para Mac.
- Suporte para leitura de texto, artigos e podcasts com síntese de voz.
- Suporte OCR PDF em macOS, descarregamento de atualizações e pacotes DMG dedicados.
- Categorias de podcast hierárquicas e primeiros atalhos globais/macOS.
