# Pazufa-Backend
This is the backend of the PaZuFa Project. It is responsible for merging
and sanity-checking the data the scrapers send in. It also provides a
public API to read the dataset of parliamentary proceedings.

# Setup Instructions
## One-Step Setup For Scraper Development
To just "get it up and running", e.g. locally as a counterpart during 
scraper development, please run:
`docker compose up -d --build` (every time you want the service to start)

`-d` is for detach, logs are then visible via 
`docker logs -f <container>`, it can be stopped via 
`docker stop <container>`, the `<container>` you can get via `docker ps`.

It will be available on http://localhost:8080 and the `ltzf-api-key` you
need is `tegernsee-apfelsaft-co2grenzwert`. Note that this key has full
permissions within the backend and can push past some sanity checks.
If you want to specifically test with a limited scope to catch bugs in 
your implementation, proceed as follows:

- run `curl https://localhost:8080/api/v2/auth -X POST -d '{"scope": "collector"}' -H "Content-Type: application/json" -H "x-api-key: tegernsee-apfelsaft-co2grenzwert"`
- take the key your backend generated, and plug it into `ltzf-api-key`

Please also note that this requires port 8080 not to be occupied by any other
program. If you wish to utilize a different port you must set it in the
`docker-compose.yml` itself by changing this line:
`- 8080:8080` to `-<your port>:8080`.

## Setting up for Development
### Setting up Rust
Set up rustc, cargo etc as described [here](https://rustup.rs/).

### Database Setup
The project currently only works with postgres. Thus, set up a postgres 
database and specify the environment variables as seen in the .env file, 
with changes as pertaining to your specific database setup.

The shortcut is to run the database service from docker compose:  
`docker compose up -d database`

### Setting up SQLX
#### Installing
Sqlx is the tool and framework we use to set up, manage and connect to 
the database. Because sqlx is not only a crate, but also a command line 
tool to run database setup, migration and all those things, you need to 
install the tool separately.

To set up sqlx, run:  
`cargo install sqlx_cli --no-default-features --features postgres`. 

If there is an error like 
```
note: ld: library not found for -lpq
clang: error: linker command failed with exit code 1 (use -v to see invocation)
```
Install the Postgres C libraries and rerun the command. 
On Debian based systems this could for example be: `sudo apt install libpq-dev`.
More information on this process can be found [here](https://sqlx.dev/article/How_to_install_SQLX.html).

### Running Instructions
If the backend detects a fresh database state it will automatically 
apply all migrations at runtime.

### Development Instructions
SQLX does compile-time query checking. This means you will need a
configured database state that already contains all the tables and 
structure _before the program is run for the first time_.

To set up the database for development you will need to run:  
`sqlx database setup`

To reset the database to the empty-but-configured state, run:  
`sqlx database reset -f`.

## Building and running the project
1. make sure your cwd is this directory
2. fetch the track file you want to use with this server. For example: 
   `curl -o tracks.toml https://codeberg.org/PaZuFa/parlamentszusammenfasser/raw/commit/32c125523ec1831edb1fdb73de5dd99d313dae9b/docs/specs/tracks.toml`
   please take the latest / Docker-specified version from `variables.toml`
3. run `sh oapigen.sh` to generate the server portion from the OpenAPI 
   spec at https://codeberg.org/PaZuFa/parlamentszusammenfasser/src/branch/main/openapi.yml
4. run `cargo build` to build the project
5. `cargo run` to run it.  Easy!

You can configure the backend either via command line arguments or via 
environment variables. This  project uses the `dotenv` crate, so 
configuring via .env files is very much the way to go if you have a 
complex configuration and want to run this in your directory.

## Bumping Versions and Paths
There is a unified mechanism for setting these variables to make sure no
important file is forgotten. The mechanism works as follows:
1. The source of truth for these values is `variables.toml`
2. The idempotent way of setting these is `set-version.py`

That means to i.e. set a new Path to a track file which should propagate
to all relevant files, you:  
1. set the variable `TRACK_FILE` in `variables.toml` to your value
2. run `python3 set-version.py`

if you want to introduce a new value that should be set in this way:
1. set the new variable in `variables.toml`
2. modify `set-version.py` by specifying the affected file, the regex
   that should be matched to replace and the value that you just set
3. run `python3 set-version.py`

**!THIS INCLUDES THE `Cargo.toml` VERSION NUMBER!**
