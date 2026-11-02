# unicursal

Single-path (unicursal) labyrinths whose solution, once filled in, reveals a
hidden picture — with grid, curvy, and organic "flow" styles, sized for real
pens, markers and crayons.

This is the Python prototype of the generator, ahead of an app build.
**Start with [HANDOFF.md](HANDOFF.md)**: how it works, the judgement calls that
should become automatic, what to expose to users, and porting notes.

```
uv sync                                # plus rsvg-convert (librsvg) on PATH
uv run picmaze.py shapes/ghost.png --flow --corridor 7 --seed 3 --fill "#8fa6d6" --out ghost
uv run pytest -q
```

Puzzle + answer-key examples are in [`examples/`](examples/).

## License

[GNU AGPL v3 or later](LICENSE).
