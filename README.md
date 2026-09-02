# RbxTools

RbxTools é um aplicativo Windows com vários utilitários de sistema.

## Primeiro utilitário: bloquear a rede da Steam

O MVP permite bloquear e liberar o tráfego de **entrada** e **saída** dos
executáveis da Steam usando regras do Windows Defender Firewall. Ele não instala
driver de rede e não altera regras que não tenham sido criadas pelo próprio
RbxTools.

> Este primeiro MVP faz bloqueio total. Controle de velocidade e limites por
> conexão, como os recursos avançados do NetLimiter, exigirão uma implementação
> futura baseada na Windows Filtering Platform (WFP).

### Executar o MVP

1. Abra o Windows PowerShell.
2. Na pasta do projeto, execute:

   ```powershell
   powershell -ExecutionPolicy Bypass -File .\src\RbxTools.ps1
   ```

3. Confirme a solicitação de administrador do Windows.
4. Confira os executáveis detectados e use **Bloquear Steam** ou **Liberar Steam**.

O botão **Liberar Steam** remove exclusivamente as regras do grupo
`RbxTools - Steam Block`.

## Requisitos

- Windows 10 ou 11
- Windows Defender Firewall habilitado
- Windows PowerShell 5.1 ou superior
- Permissão de administrador para alterar regras do firewall

## Estrutura

- `src/RbxTools.ps1`: interface gráfica e integração com o firewall
- `docs/architecture.md`: evolução recomendada para o aplicativo

## Segurança

Revise os caminhos exibidos antes de bloquear. O projeto não captura pacotes,
credenciais ou conteúdo de rede. Consulte [SECURITY.md](SECURITY.md) para relatar
problemas de segurança.

