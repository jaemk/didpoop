> track that poop

### setup

```shell
cp .env.sample .env
createuser didpoop -P
createdb -O didpoop didpoop
migrant setup
migrant apply -a
```

### run

```shell
cargo run
```

### build

```shell
./docker.sh build
```
