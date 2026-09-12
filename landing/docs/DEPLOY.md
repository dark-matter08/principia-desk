# Deploying principia.ndelucien.com

The box is `173.249.55.72`, the one running `vertex`, `ezz`, `ndelucien`, `sb` and `trl`. This site is one more stack on it, the smallest kind: an nginx container serving static files, capped at 0.5 CPU and 128 MB, reached through the box-wide `vertex_nginx`.

## How it is wired

- **Image** `ghcr.io/dark-matter08/principia-landing`, built by `.github/workflows/landing-ci.yml` from the repository root with `landing/docker/Dockerfile.web.prod` (the changelog and licence pages read `CHANGELOG.md` and `LICENSE` from the root).
- **Stack** `principia`, one service `principia_web` on port 9940, on its own `internal` overlay and on `vertex_web`, from `landing/docker/docker-stack.production.yml` copied to `/opt/principia/production/docker/`.
- **Ingress** `landing/docker/nginx-principia.conf`, installed as `/opt/ndelucien/production/nginx-ndelucien.d/principia.conf`. That directory is already included by `vertex_nginx` (`include /etc/nginx/ndelucien.d/*.conf;`, mounted in the vertex-reader stack file), so nothing changes in the vertex repository for this site.
- **Certificate** ndelucien.com's, expanded to cover `principia.ndelucien.com` on 2026-09-12. `make -C landing cert` runs the expansion again if it is ever reissued without the name.
- **DNS** an A record `principia.ndelucien.com → 173.249.55.72`.

## The deploy, step by step

`.github/workflows/landing-deploy.yml` runs after Landing CI on `main`, or by hand with a tag (`sha-abc1234` rolls back). It:

1. copies the stack file and the ingress block to `/opt/principia/production/docker/`;
2. installs the ingress block **only if the certificate names the host** (a server block naming a certificate that does not cover it fails `nginx -t`, and the next reload for any reason would refuse and take every site down);
3. logs in to GHCR with `GHRC_TOKEN`, pulls the image explicitly and hard-fails if it cannot (`stack deploy` would otherwise ship whatever is on the box and report success);
4. `docker stack deploy --with-registry-auth --resolve-image=always principia`;
5. waits until the running task's image tag is the one asked for, then fetches the page from inside the running container and checks the product name is in it;
6. runs `nginx -t` in `vertex_nginx` and reloads it only if the config tests clean;
7. checks the public URL, and prunes old images.

Secrets on the repository: `PRODUCTION_SSH_HOST`, `PRODUCTION_SSH_USER`, `PRODUCTION_SSH_KEY`, `GHRC_TOKEN` (a token with `read:packages`, to pull the private package).

## By hand

```bash
cd landing
make cert                  # once, after the A record exists
make push                  # build the image here and push it (needs write:packages)
make deploy-production     # scp the stack + ingress, docker stack deploy
make rollout-status
```

`--resolve-image=always` still skips a service when the stack file is unchanged and the stored digest matches; `make force-update` when you know the image moved. The first deploy (2026-09-12) was built on the box itself from a tarball of `landing/`, `CHANGELOG.md` and `LICENSE`, and deployed with `--resolve-image=never`, because the token on the Mac can read packages but not write them; the workflow's own token can.

## Traps, all learned on this box

- **Fully qualified upstreams.** `vertex_nginx` is the only container on `vertex_web` without a second network membership; a bare `web` resolves to whichever stack answers first. Our upstream is `principia_web:9940`.
- **File modes in the ingress directory.** The nginx worker runs as uid 101; root's umask makes new files 0600. The deploy sets 755 on the directory and 644 on the file every time.
- **Nothing on the box by hand that the vertex repository owns.** The include and the mount for `ndelucien.d` live in vertex-reader; a vertex deploy rewrites the box-wide nginx from that repository. This site only adds a file inside a directory that repository already mounts.
