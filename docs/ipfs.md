# Running IPFS node

This guide explains how to run IPFS node in resource-constrained environment (such as cheap VPS or single-board computer).

The recommended IPFS implementation is [go-ipfs](https://github.com/ipfs/go-ipfs), version 0.11 or higher. Normally go-ipfs requires at least 2 GB RAM, but after tweaking it can run on a machine with only 512 MB.

## Configuration profiles

IPFS configuration should be initialized with `server` profile if your node is running on a cloud server:

```
ipfs init --profile server
```

If you're running it on single-board computer, the recommended profile is `lowpower`.

Documentation on configuration profiles: https://github.com/ipfs/go-ipfs/blob/master/docs/config.md#profiles.

## Configuration options

- `Gateway.NoFetch`. Configures gateway to not fetch files from the network. Recommended value is `true`.
- `RelayService.Enabled`. Enables providing p2p relay service to other peers on the network. Recommended value is `false`.
- `Routing.Type`. Should be set to `dht` otherwise the node will not respond to requests from other peers.
- `Swarm.ConnMgr.LowWater`. Recommended value is `10`.
- `Swarm.ConnMgr.HighWater`. Recommended value is `20`.
- `Swarm.ConnMgr.GracePeriod`. Recommended value is `15s`.
- `Swarm.DisableBandwidthMetrics`. Disabling bandwidth metrics can slightly improve performance. Recommended value is `true`.

Documentation: https://github.com/ipfs/go-ipfs/blob/master/docs/config.md

## Systemd service

When go-ipfs starts, its memory usage is around 100 MB and then it slowly increases. To keep memory usage within reasonable bounds the service needs to be restarted regularly.

This can be achieved by using systemd process supervison features:

```
[Unit]
Description=InterPlanetary File System (IPFS) daemon

[Service]
ExecStart=/usr/local/bin/ipfs daemon
User=ipfs
Group=ipfs

# Terminate service every 20 minutes and restart automatically
RuntimeMaxSec=1200
Restart=on-failure

# Specify the absolute limit on memory usage
# If memory usage cannot be contained under the limit, out-of-memory killer is invoked
MemoryMax=250M
MemorySwapMax=0

[Install]
WantedBy=default.target
```

Documentation:

- https://www.freedesktop.org/software/systemd/man/systemd.service.html
- https://www.freedesktop.org/software/systemd/man/systemd.resource-control.html
