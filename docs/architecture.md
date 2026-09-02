# Arquitetura proposta

## MVP

O protótipo atual usa PowerShell e o módulo `NetSecurity`, já incluído no
Windows, para validar rapidamente a experiência:

1. detectar os executáveis principais da Steam;
2. mostrar ao usuário exatamente quais caminhos serão afetados;
3. criar uma regra de bloqueio de entrada e outra de saída para cada executável;
4. remover apenas as regras pertencentes ao grupo do RbxTools.

## Aplicativo nativo

A próxima etapa recomendada é migrar a interface para .NET 8 com WPF, mantendo
as operações privilegiadas em um componente pequeno e isolado. A interface não
deve permanecer elevada durante todo o uso.

```text
Interface WPF (usuário normal)
        |
        | pedido validado
        v
Helper elevado e restrito
        |
        v
Windows Firewall / WFP
```

Para bloqueio total, as APIs do Windows Firewall são suficientes. Para limitar
velocidade, inspecionar conexões em tempo real ou aplicar políticas mais
granulares, será necessário estudar a Windows Filtering Platform (WFP),
assinatura de código e distribuição segura do componente privilegiado.

