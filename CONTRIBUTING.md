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
milestone and a project.

### Labelling
Issues all receive one of the following labels:
- bug: Some failure occurred or Error was observed, and the backend is
  at fault
- enhancement: There is a proposition for improvement, features or other
  additions to functionality

And they may receive one of the following, depending on the context:
- discussion: This is not a one-and-done thing, but requires
  consideration
- wontfix: We will not further work on this issue
- duplicate: the issue already exists elsewhere

Depending on the context, we might assign special issue labels.

## Testing
To test the project, you need a version of the `tracks.toml` file to 
construct the Server and the track validation successfully.

NOTE: you need the file twice, currently: once in 
`./pazufa-backend` and once in `./pazufa-backend-lib`


If errors occur that are most likely narrowed down to that file, please
fetch the version specified in `./variables.toml`
Convenience command to fetch one and put it in all the right spots:  
`curl -o pazufa-backend/tracks.toml https://codeberg.org/PaZuFa/parlamentszusammenfasser/raw/tag/v0.2.3+0.0.5/docs/specs/tracks.toml && cp pazufa-backend/tracks.toml pazufa-backend-lib`

## Version Control
We work with three long-running branches:
- main: our main release branch. Ideally only stable features land here.
- develop: our dirty branch to develop stuff on. Should be rebased to
  reflect a sane development history for main

NOTE that we check the .woodpecker/staging.yml workflow on each PR to
main. This mostly tries to build the docker image once, and checks with
docker hub whether the tag (=version) already exists.

- PRs from develop onto main should be FF Only
- PRs from feature branches onto develop should be squash only

### Normal development
When working on an issue, please do it in a new branch either here or in
a fork, which after full implementation of all required features is 
PR'd into **develop** with a **squash commit**, closing the issue.

Occasionally one of the maintainers might rebase and squash the develop 
git historyto reflect a solid development history pattern.

After a milestone has been reached, all commits on develop are 
fast-forwarded to main, a release tag and message 
is written and the docker images are built and pushed to the hub.

### Version Bumping
A version is bumped as a first commit after a new release, which is
commited to develop.
This is done by: 
- changing the version number in `variables.toml`
- running `python3 set-version.py`

### Hotfixes
If a hotfix is required for main, main is branched into a respective
`hotfix/...` branch. On this branch, the hotfix is implemented.
Subsequently, the hotfix branch is then squashed into main with a 
patch-version bump, a new release and a new  docker image.

Then staging and develop are rebased on main to reflect the hotfix.


### My branch has conflicts, hälp
If you get a message telling you about branch conflicts, the most likely
cause is a missing rebase from develop onto your feature branch. In that
case, a `git rebase develop` while on your branch should suffice.

A different cause could be that someone squashed some git commits 
together to make the history make sense.
In plain english, you are missing some commits from develop, and based
your changes on an older version, which is now outdated.

THIS CAN BE FIXED!

The solution is to take this new "base version", and reapply your
changes onto that base version.

Do not panic. There are two ways to replay your commit history onto this
new base history:

1. find out which changes are from the old develop branch, and which are
   added by yourself on your new branch. (try `git log --oneline` for
   example. At which point did you start adding commits to this branch?
   Which changes are already in develop?)
2. using cherry-picking (for only a few commits that are new)
    1. checkout a new branch based on the current version of develop:
       `git switch develop && git pull && git checkout -b new-branch`
    2. cherry-picking your changes on top of this fresh 
       branch: `git cherry-pick 00001`, `git cherry-pick 00002`, ... 
       until you are at the end of your "old" branch
3. using rebase --onto, which makes for a smoother fixing process, if
   you otherwise had to cherry-pick 10 commits or something:
    1. `git rebase --onto develop <last commit _before_ your feature branch> <your feature branch's name>`
        this reads as: "use develop as a new base history for my branch.
        Take all commits on "your feature branch..." _after and not including_ this "last commit...", and
        replay it onto this new base history"
    2. walk through the rebase as you would with any other rebase
4. then delete your old feature branch: `git branch -D <old-branch-name>`
5. and you can continue developing and PR'ing into develop

What you then did was take the updated history from develop and re-apply
your feature-branch-commits on top.

