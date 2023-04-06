# Running in Containers
In case you want to run radicle in containers, on the same host (e.g. your laptop),
you can use the `docker-compose.yml` file provided within this repo. 

## 1. Create a profile 

1. Create a folder where you will store the data of your node. e.g. `mkdir -p ~/radicle/profiles/bob/.radicle`
1. Set `RAD_HOME` : `export RAD_HOME=~/radicle/profiles/bob/.radicle`
1. Create a key:
   - Pick a good passphrase and store it in your password manager
   - go ahead with creating the key `rad auth --stdin` (or use `RAD_PASSPHRASE` env var)
   - your profile should be created in `~/radicle/profiles/bob/.radicle`. 

## 2. Build the container images

This takes a couple of minutes - depending on your machine - as it needs 
to download the parent container images, and also all the rust 
dependencies and then compile the code: 

```bash
docker-compose build
```

## 3. Start the containers 

This is as simple as using the existing `docker-compose.yml` and simply passing in 
the path to the **parent** folder of `RAD_HOME`, where you previously created the 
profile, as per the instructions in #1 above.

```bash
RAD_ROOT=~/radicle/profiles/bob docker-compose up radicle-node radicle-httpd 
```