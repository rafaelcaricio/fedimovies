# Monero integration

Install Monero node or choose a [public one](https://monero.fail/).

Install and configure [monero-wallet-rpc](https://monerodocs.org/interacting/monero-wallet-rpc-reference/) service. Add `disable-rpc-login=1` to your `monero-wallet-rpc` configuration file (currently RPC auth is not supported in Mitra). See [example](../contrib/monero/wallet.conf).

Start `monero-wallet-rpc`. Create a wallet for your instance:

```
mitractl create-monero-wallet "mitra-wallet" "passw0rd"
```

Add blockchain configuration to `blockchains` array in your configuration file.
