"""Read the world map's node coordinates out of the compiled game.

They are not in data.win. UI_Map_Screen's create event builds them inline, so
this disassembles that one function and keeps the doubles that look like screen
coordinates. See README.md for why that works and how to regenerate the inputs.
"""
import struct, json, bisect, csv, io, sys
from capstone import *
from capstone.x86 import *
sys.stdout.reconfigure(encoding='utf-8')
P=r'F:\Games\Steam\steamapps\common\HeroSiege\linux\Hero_Siege'
raw=open(P,'rb').read(); BASE=0x400000
syms=json.load(open('syms.json')); rooms=json.load(open('rooms.json'))
addrs=sorted(set(syms.values()))
md=Cs(CS_ARCH_X86,CS_MODE_64); md.detail=True
fn=syms['gml_Script_anon@2078@gml_Object_UI_Map_Screen_obj_Create_0']
END=addrs[bisect.bisect_right(addrs,fn)]
ins_list=list(md.disasm(raw[fn-BASE:END-BASE],fn))
def b2d(v): return struct.unpack('<d',struct.pack('<Q',v&0xFFFFFFFFFFFFFFFF))[0]
def ok(d): return d==d and 1.0<=d<=3000.0 and abs(d-round(d))<1e-9
ASSET=0x100000300000001
line=None; regs={}; nodes={}
for k,ins in enumerate(ins_list):
    ops=ins.operands
    if ins.mnemonic=='mov' and '0x1ab0]' in ins.op_str and ops and ops[-1].type==X86_OP_IMM:
        line=ops[-1].imm; continue
    if ins.mnemonic=='movabs' and len(ops)==2 and ops[0].type==X86_OP_REG and ops[1].type==X86_OP_IMM:
        v=ops[1].imm; r=ops[0].reg
        if v==ASSET:
            nxt=ins_list[k+1]; disp=0
            if nxt.mnemonic in ('lea','add'):
                for op in nxt.operands:
                    if op.type==X86_OP_MEM and op.mem.base!=X86_REG_RIP: disp=op.mem.disp
                    if op.type==X86_OP_IMM: disp=op.imm
            nodes.setdefault(line,{})['room']=disp+1; regs.pop(r,None)
        else:
            d=b2d(v); regs[r]=round(d) if ok(d) else None
        continue
    if ins.mnemonic=='mov' and len(ops)==2 and ops[0].type==X86_OP_MEM and ops[1].type==X86_OP_REG:
        val=regs.get(ops[1].reg)
        if val is not None and ops[0].mem.base==X86_REG_RSP:
            nodes.setdefault(line,{}).setdefault('xy',[]).append(val)
        continue
    if ops and ops[0].type==X86_OP_REG and ins.mnemonic not in ('cmp','test','bt','push'):
        regs.pop(ops[0].reg,None)
zn={}
for row in io.open(r'F:\Games\Steam\steamapps\common\HeroSiege\bin\translationsZone.csv',encoding='utf-8',errors='replace'):
    p=row.rstrip('\n').split('|')
    if len(p)>1: zn[p[0]]=p[1]
res=[]
for l in sorted(x for x in nodes if x is not None and 'room' in nodes[x]):
    v=nodes[l]; xy=v['xy']; ri=v['room']; rm=rooms[ri] if 0<=ri<len(rooms) else f'?{ri}'
    res.append({'gml_line':l,'x':xy[0],'y':xy[1],'room_index':ri,'room':rm,'zone_name':zn.get(rm,'')})
assert all(len(nodes[l]['xy'])==2 for l in nodes if 'room' in nodes[l])
io.open('map_nodes.json','w',encoding='utf-8').write(json.dumps(res,ensure_ascii=False,indent=1))
with io.open('map_nodes.csv','w',encoding='utf-8',newline='') as f:
    w=csv.writer(f); w.writerow(['gml_line','x','y','room_index','room','zone_name'])
    for r in res: w.writerow([r['gml_line'],r['x'],r['y'],r['room_index'],r['room'],r['zone_name']])
print('nodes:',len(res),'| x',min(r['x'] for r in res),'-',max(r['x'] for r in res),
      '| y',min(r['y'] for r in res),'-',max(r['y'] for r in res))
for r in res[:12]: print(f"{r['x']:5},{r['y']:4}  {r['room']:26} {r['zone_name']}")
print('...')
for r in res[-4:]: print(f"{r['x']:5},{r['y']:4}  {r['room']:26} {r['zone_name']}")
