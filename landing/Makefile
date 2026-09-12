# principia.ndelucien.com — build, push, deploy. One deploy shape with
# theremoteledger.org and ndelucien.com on the same box.

PRODUCTION_HOST ?= root@173.249.55.72
GHCR_OWNER      ?= dark-matter08
IMAGE           := ghcr.io/$(GHCR_OWNER)/principia-landing
IMAGE_TAG       ?= production
STACK           := principia
REMOTE_DIR      := /opt/principia/production
INGRESS_DIR     := /opt/ndelucien/production/nginx-ndelucien.d
DOMAIN          := principia.ndelucien.com

.PHONY: dev build check image push deploy-production rollout-status force-update logs cert

dev:
	npm run dev

build:
	npm run build

check:
	npx astro check && npm run build

## The build context is the repository root: the changelog and licence pages
## read ../CHANGELOG.md and ../LICENSE.
image:
	cd .. && docker build -f landing/docker/Dockerfile.web.prod -t $(IMAGE):$(IMAGE_TAG) .

push: image
	docker push $(IMAGE):$(IMAGE_TAG)

deploy-production:
	@echo "Deploying $(DOMAIN) to $(PRODUCTION_HOST)..."
	ssh $(PRODUCTION_HOST) "mkdir -p $(REMOTE_DIR)/docker $(INGRESS_DIR) && chmod 755 $(INGRESS_DIR)"
	scp docker/docker-stack.production.yml $(PRODUCTION_HOST):$(REMOTE_DIR)/docker/docker-stack.production.yml
	scp docker/nginx-principia.conf        $(PRODUCTION_HOST):$(INGRESS_DIR)/principia.conf
	ssh $(PRODUCTION_HOST) "chmod 644 $(INGRESS_DIR)/principia.conf; export GHCR_OWNER=$(GHCR_OWNER) IMAGE_TAG=$(IMAGE_TAG); \
		docker stack deploy -c $(REMOTE_DIR)/docker/docker-stack.production.yml --with-registry-auth --resolve-image=always $(STACK)"
	@echo "Deployed. Check with: make rollout-status"

rollout-status:
	ssh $(PRODUCTION_HOST) "docker service ps $(STACK)_web --no-trunc | head -5; \
		docker service inspect $(STACK)_web --format '{{.Spec.TaskTemplate.ContainerSpec.Image}}'"

## `--resolve-image=always` still skips a service when the YAML text is
## unchanged and the stored digest matches. Force it when the image moved.
force-update:
	ssh $(PRODUCTION_HOST) "docker service update --force --image $(IMAGE):$(IMAGE_TAG) \
		--update-failure-action rollback --detach=false $(STACK)_web"

logs:
	ssh $(PRODUCTION_HOST) "docker service logs -f --tail 100 $(STACK)_web"

## One-time, after the DNS A record exists and BEFORE the first deploy installs
## the server block: expand ndelucien.com's certificate to this name. certbot
## fails the whole request if any name in it cannot be validated.
cert:
	ssh $(PRODUCTION_HOST) "certbot certonly --webroot -w /var/www/certbot --cert-name ndelucien.com \
		-d ndelucien.com -d admin.ndelucien.com -d api.ndelucien.com -d supabase.ndelucien.com -d s3.ndelucien.com \
		-d $(DOMAIN) --expand"
