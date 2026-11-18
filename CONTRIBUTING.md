# PaZuFa Contribution Guidelines

If you want to contribute to this repository, thank you you glorious
piece of human society. Please note some conventions we try to adhere
to, to keep the overall process sane and followable.

## Issues

Issues should be scoped to this repository. Please note some explicit
exceptions:

- Issues with the overall specification
- Issues with tracks.toml
These two should be put
[here](https://codeberg.org/PaZuFa/parlamentszusammenfasser/issues/new),
because the backend is only executing what the specification is saying.
If however the problem is in the implementation, you are correct in
filing an issue here.
If you have filed an issue, we will label it accordingly, assign it a
milestone and labels.

### Labelling
All issues receive one of the following labels:
- bug: Some failure occurred or Error was observed, and the backend is
  at fault
- enhancement: There is a proposition for improvement, features or other
  additions to functionality

And they may receive one of the following, depending on the context:
- discussion: This is not a one-and-done thing, but requires
  consideration
- wontfix: We will not further work on this issue
- duplicate: the issue already exists elsewhere

Depending on the context, we might assign special issue labels, e.g.
project part labels or moonshot.

## Testing
To test the project, you need a version of the `tracks.toml` file to 
construct `track validation` successfully.

NOTE: you need the file three times, currently: in 
- `./pazufa-backend` 
- `./pazufa-backend-lib`
- `./pazufa-models`

If errors occur that are most likely narrowed down to that file, please
fetch the version specified in `./variables.toml`
Convenience command to fetch one and put it into all the right spots:  
```bash
curl -o pazufa-backend/tracks.toml https://codeberg.org/PaZuFa/parlamentszusammenfasser/raw/tag/v0.2.9+0.1.4/tracks.toml
cp pazufa-backend/tracks.toml pazufa-backend-lib
cp pazufa-backend/tracks.toml pazufa-models
```

## Version Control
We work with two long-running branches:
- **main**: our main release branch. Only stable features land here from
  develop or from hotfix branches.
- **develop**: the dirty branch to develop stuff on and merge things to. 
  Should be rebased to reflect a sane development history before FFing
  to main.

NOTE that we check the .woodpecker/staging.yml workflow on each PR to
main. The pipeline tries to build the docker image once, and checks with
docker hub whether the tag (=version) already exists.

- PRs from develop onto main should be FF Only
- PRs from feature branches onto develop should be squash or FF,
  depending on what is on the branch

### Normal development
When working on an issue, please do it in a new branch either here or in
a fork, which after full implementation of all required features is 
PR'd into **develop** with a **squash commit**, closing the issue.

Occasionally one of the maintainers might rebase and squash the develop 
git history to reflect a solid development history pattern.

After a milestone has been reached, all commits on develop are 
fast-forwarded to main, a release tag and message is written and the 
docker images are built and pushed to the hub.

### Changing Versions and URLs
There is a unified mechanism for setting versions and shared variables 
to make sure no important file is forgotten. The mechanism works as follows:
1. The source of truth for these values is `variables.toml`
2. The idempotent way of setting these is `set-version.py`

That means to e.g. set a new path to the swagger ui cache which should 
propagate to all relevant files, you:  
1. set the variable `resources.cached-swagger-ui` in `variables.toml` to 
  the new value
2. run `python3 set-version.py`

if you want to introduce a new value that should be set in this way,
e.g. a new crate that should be versioned:
1. add the new variable in `variables.toml`
2. modify `set-version.py` by specifying the affected file, the regex
   that should be matched to replace and the value that you just set
3. run `python3 scripts/set-version.py`

**!ALL VERSIONS ARE SET THIS WAY, INCLUDING THE `Cargo.toml` VERSION NUMBERS!**

### Hotfixes
If a hotfix is required for main, main is branched into a respective
`hotfix/...` branch. On this branch, the hotfix is implemented.
Subsequently, the hotfix branch is then squashed into main with a 
patch-version bump, a new release and a new  docker image.

Then staging and develop are rebased onto main to reflect the hotfix.

### My branch has conflicts, hälp
If you get a message telling you about branch conflicts, the most likely
cause is a missing rebase from develop onto your feature branch. In that
case, a `git rebase develop` while on your branch should suffice.

A different cause could be that someone squashed some git commits 
together to make the history make sense or rebased main and altered a
few commits.
In plain english, you are missing some commits from develop, and based
your changes on an older version of that branch, which is now outdated.

THIS CAN BE FIXED!

The solution is to take this new "base version", and reapply your
changes onto that base version.

Do not panic. There are two ways to replay your commit history onto this
new base history:

For both, first find out which changes are from the old develop branch, 
and which are added by yourself on your new branch. (try `git log --oneline`
for example) 
Answer these questions: 
- At which point did you start adding commits to this branch?
- Which changes are already in develop?

Options:
1. using cherry-picking (for only a few commits that are new)
    1. checkout a new branch based on the current version of develop:
       `git switch develop && git pull && git checkout -b new-branch`
    2. cherry-picking your changes on top of this fresh 
       branch: `git cherry-pick 00001`, `git cherry-pick 00002`, ... 
       until you are at the end of your "old" branch
    3. then delete your old feature branch: `git branch -D <old-branch-name>`

2. using rebase --onto you can keep your old branch, if you otherwise 
  had to cherry-pick 10 commits or something:
    1. `git rebase --onto develop <last commit _before_ your feature branch> <your feature branch's name>`
        this reads as: "use develop as a new base history for my branch.
        Take all commits on "your feature branch..." _after and not including_ this "last commit...", and
        replay it onto this new base history"
    2. walk through the rebase as you would with any other rebase


What you then did was take the updated history from develop and re-apply
your feature-branch-commits on top.

