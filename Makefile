.PHONY: build run stop logs clean test

# Build Docker image
build:
	docker build -t zggsds:latest .

# Run container
run:
	docker run -d \
		--name zggsds \
		-p 3000:3000 \
		-v $(PWD)/data:/app/data \
		-e RUST_LOG=info \
		--restart unless-stopped \
		zggsds:latest

# Stop container
stop:
	docker stop zggsds || true
	docker rm zggsds || true

# View logs
logs:
	docker logs -f zggsds

# Remove image and containers
clean: stop
	docker rmi zggsds:latest || true

# Rebuild and restart
rebuild: clean build run

# Run tests
test:
	cargo test

# Show container status
status:
	docker ps | grep zggsds || echo "Container not running"
