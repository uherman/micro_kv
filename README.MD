# micro-kv

A minimal key-value database written in Rust.

## Quickstart

### Docker

```bash
docker run mioherman/micro-kv:latest -p 3310:3310 --name micro-kv -d
```

### Compile and run binary

```bash
cargo build --release
```

```
./target/release/micro-kv
```

## Usage

### Set a key

```bash
curl --request POST \
  --url 'http://localhost:3310/123?ttl=60' \
  --header 'Content-Type: application/json' \
  --data '{
	"_id": "123",
	"name": "John Doe"
}'
```

### Get a key

```bash
curl --request GET \
  --url 'http://localhost:3310/123'
```

### Get all keys

```bash
curl --request GET \
  --url 'http://localhost:3310'
```

### Get TTL of a key

```bash
curl --request GET \
  --url 'http://localhost:3310/ttl/123'
```

### Delete a key

```bash
curl --request DELETE \
  --url 'http://localhost:3310/123'
```

## Contributing

Pull requests are welcome. For major changes, please open an issue first to discuss what you would like to change.

### Conventional Commits

Please use [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/) for commit messages.

## License

[MIT](./LICENSE)
