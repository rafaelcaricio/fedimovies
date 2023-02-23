# Tor instance

Install Tor.

Install Mitra. Uncomment or add the following block to Mitra configuration file:

```yaml
federation:
  proxy_url: 'socks5h://127.0.0.1:9050'
```

Where `127.0.0.1:9050` is the address and the port where Tor proxy is listening.

Configure the onion service by adding these lines to `torrc` configuration file:

```
HiddenServiceDir /var/lib/tor/mitra/
HiddenServicePort 80 127.0.0.1:8383
```

Where `8383` should correspond to `http_port` setting in Mitra configuration file.

Restart the Tor service. Inside the `HiddenServiceDir` directory find the `hostname` file. This file contains the hostname of your onion service. Change the value of `instance_uri` parameter in Mitra configuration file to that hostname (it should end with `.onion`).

Start Mitra.

For more information about running onion services, visit https://community.torproject.org/onion-services/setup/
