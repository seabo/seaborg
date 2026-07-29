import sys; sys.path.insert(0, "/home/seabo/rl/seaborg/tools/trainer")
import torch, numpy as np
from pathlib import Path
from export import QuantizedNetwork
from model import NnueModel, NnueConfig, ACTIVATION_IDS
_ID2ACT = {v: k for k, v in ACTIVATION_IDS.items()}
from data import PackedData, BatchLoader
from split import load_manifest, by_shard_split
import train

def load_model(path):
    q = QuantizedNetwork.from_bytes(Path(path).read_bytes())
    H = q.hidden
    act = q.activation if isinstance(q.activation, str) else _ID2ACT[int(q.activation)]
    cfg = NnueConfig(hidden=H, activation=act, qa=q.qa, qb=q.qb, scale=q.scale)
    m = NnueModel(cfg, quantization_aware=True)
    sd = m.state_dict()
    sd["feature_transformer.weight"] = torch.tensor(q.w_ft.reshape(768, H).astype(np.float32) / q.qa)
    sd["ft_bias"] = torch.tensor(q.b_ft.astype(np.float32) / q.qa)
    sd["output.weight"] = torch.tensor(q.w_out.reshape(1, 2 * H).astype(np.float32) / q.qb)
    sd["output.bias"] = torch.tensor(q.b_out.astype(np.float32) / (q.qa * q.qb))
    m.load_state_dict(sd)
    return m, q.scale

D = "/home/seabo/rl/corpus-gen-002"
data = PackedData(D + "/corpus.bin")
s = by_shard_split(load_manifest(D + "/corpus.manifest.json"), val_fraction=0.1, seed=0)
dev = "cuda"
nets = [
    ("gen-002 (shipped, old bootstrap data)", "/home/seabo/rl/seaborg/engine/nets/default.sbnn"),
    ("retrained h256 (new corpus)",           "/home/seabo/rl/sweep-86.5/nets/baseline__h256_crelu_v1.sbnn"),
]
for name, path in nets:
    m, scale = load_model(path); m.to(dev)
    with BatchLoader(data, 8) as loader:
        vl = train._evaluate(m, loader, s.val_idx, 8192, dev, scale, 0.3)
    print("%-40s val_loss=%.6f" % (name, vl))
