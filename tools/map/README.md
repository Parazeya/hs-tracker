# Where the map's nodes are

The 63 places on Hero Siege's world map, with the coordinates the game draws them
at. `map_nodes.csv` is the result; the rest is how it was got and how to get it
again after a patch.

| file | what it is |
| --- | --- |
| `map_nodes.csv` | the answer: `gml_line, x, y, room_index, room, zone_name` |
| `map_nodes.json` | the same, with the raw operands each number came from |
| `extract_map_nodes.py` | reads them back out of the Linux binary |
| `rooms.json` | room index to room name, from `data.win` |
| `syms.json` | name to address, from GameMaker's own registry |

The coordinates are not in `data.win`. They are written in the compiled code, in
`UI_Map_Screen`'s create event, as a run of doubles fed to a struct literal — one
node per iteration, each with an x, a y and the room it opens. So they are read
by disassembling that one function and watching what it loads: `ok()` accepts a
double that is a whole number in 1..3000, which is what a screen coordinate looks
like and what a float register holding something else almost never does.

`syms.json` is the key to finding the function at all. A YYC build has no `CODE`
chunk to look in, but `.data` still holds GameMaker's registry of its own variable
and function names — records of `{const char *name; i32 id; i32 flags}` — and
`gml_Script_anon@2078@gml_Object_UI_Map_Screen_obj_Create_0` is in it by name.

Room indices are 0-based, which is checked rather than assumed: all 64 rows of
`map_nodes.csv` resolve through `rooms.json` to the name recorded beside them.
64 rows, 63 places — Cabin_01_rm is the tutorial and is not one of them.

## Running it again

Needs `capstone`, the Linux build at the path in the script, and both JSONs
beside it. `rooms.json` comes from `datawin.py`'s ROOM chunk; `syms.json` from
the registry walk described above.
