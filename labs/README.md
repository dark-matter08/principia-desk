# Principia Desk shell labs

The Linux Bash and Bash Scripting courses target a learner-controlled Linux environment with Bash 5.x and GNU utilities. A Linux machine, VM, container or WSL distribution can provide that environment. Native macOS commands can differ from their GNU counterparts; record the environment used for your work.

These first reference experiments only print observations and return statuses. They create no files, need no credentials or elevated permissions, and run outside Principia Desk. The app does not execute learner commands.

From the repository root:

```bash
bash labs/linux-bash/lb-quoting.sh
bash labs/bash-scripting/bs-error-handling.sh
```

In the quoting experiment, compare `argc=1` with `argc=2`, and distinguish a single empty argument from zero arguments. In the failure experiment, the pipeline statuses should be `0` then `1`; `report failed: 7` goes to stderr, and `publish report` must never appear.

The same examples are bundled in the matching reference lessons. Their exercises ask you to extend the observation into a tested tool. The larger investigation fixtures, automation failure suite and capstone rubrics remain part of the curriculum completion work in the product plan.
