import argparse
import os
import time
import numpy as np
import onnx
from onnxconverter_common import float16
import onnxruntime as ort


def make_static_shapes(model: onnx.ModelProto, size: int) -> onnx.ModelProto:
    del model.graph.value_info[:]
    inp = model.graph.input[0]
    dims = inp.type.tensor_type.shape.dim
    static = [1, 3, size, size]
    for i, val in enumerate(static):
        if i < len(dims):
            dims[i].ClearField("dim_param")
            dims[i].dim_value = val
    for out in model.graph.output:
        if len(out.type.tensor_type.shape.dim) > 0:
            out.type.tensor_type.shape.dim[0].ClearField("dim_param")
            out.type.tensor_type.shape.dim[0].dim_value = 1
    return onnx.shape_inference.infer_shapes(model)


def export_true_fp16(src_path: str, dst_path: str, size: int):
    print(f"[FP16] Converting {src_path} -> {dst_path} (static [1,3,{size},{size}])...")
    model = make_static_shapes(onnx.load(src_path), size)
    model_fp16 = float16.convert_float_to_float16(
        model,
        keep_io_types=False,
        disable_shape_infer=False,
    )
    # Fix YOLOv8 Detect head (/model.22/Add_6) where Cast nodes still target FP32 (1) instead of FP16 (10)
    for node in model_fp16.graph.node:
        if node.op_type == "Cast":
            for attr in node.attribute:
                if attr.name == "to" and attr.i == onnx.TensorProto.FLOAT:
                    attr.i = onnx.TensorProto.FLOAT16
    del model_fp16.graph.value_info[:]
    onnx.save(model_fp16, dst_path)

    try:
        ort.InferenceSession(dst_path, providers=["CPUExecutionProvider"])
        mode = "True FP16-IO"
    except Exception:
        model = make_static_shapes(onnx.load(src_path), size)
        head_nodes = [n.name for n in model.graph.node if "/model.22/" in n.name and n.name]
        model_fp16 = float16.convert_float_to_float16(
            model,
            keep_io_types=True,
            disable_shape_infer=False,
            node_block_list=head_nodes,
        )
        del model_fp16.graph.value_info[:]
        onnx.save(model_fp16, dst_path)
        ort.InferenceSession(dst_path, providers=["CPUExecutionProvider"])
        mode = "Hybrid FP16 (FP32 Detect head)"

    mb = os.path.getsize(dst_path) / (1024 * 1024)
    print(f"  Saved {dst_path} ({mb:.1f} MB) [{mode}]")


def benchmark(model_path: str, size: int, provider: str, runs: int = 30):
    if not os.path.isfile(model_path):
        return
    try:
        opts = ort.SessionOptions()
        opts.graph_optimization_level = ort.GraphOptimizationLevel.ORT_ENABLE_ALL
        opts.enable_mem_pattern = (provider == "CPUExecutionProvider")
        opts.execution_mode = ort.ExecutionMode.ORT_SEQUENTIAL

        sess = ort.InferenceSession(model_path, sess_options=opts, providers=[provider])
        inp = sess.get_inputs()[0]
        dtype = np.float16 if "float16" in inp.type else np.float32
        x = np.random.rand(1, 3, size, size).astype(dtype)

        for _ in range(5):
            sess.run(None, {inp.name: x})

        t0 = time.perf_counter()
        for _ in range(runs):
            sess.run(None, {inp.name: x})
        ms = (time.perf_counter() - t0) * 1000.0 / runs
        print(f"  {os.path.basename(model_path):22s} | {provider:24s} | {ms:6.2f} ms/frame")
    except Exception as e:
        print(f"  {os.path.basename(model_path):22s} | {provider:24s} | FAILED ({e})")


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--m640", default=r"D:\Censorship\ModelBackups\640m.onnx")
    ap.add_argument("--m320", default=r"runtime\320n.onnx")
    ap.add_argument("--out-dir", default="runtime")
    ap.add_argument("--bench", action="store_true")
    args = ap.parse_args()

    os.makedirs(args.out_dir, exist_ok=True)
    fp16_640 = os.path.join(args.out_dir, "640m_fp16.onnx")
    fp16_320 = os.path.join(args.out_dir, "320n_fp16.onnx")

    if os.path.isfile(args.m320):
        export_true_fp16(args.m320, fp16_320, 320)
    if os.path.isfile(args.m640):
        export_true_fp16(args.m640, fp16_640, 640)

    if args.bench:
        print("\n=== DirectML FP16 Benchmark ===")
        benchmark(args.m320, 320, "DmlExecutionProvider")
        benchmark(fp16_320, 320, "DmlExecutionProvider")
        benchmark(args.m640, 640, "DmlExecutionProvider")
        benchmark(fp16_640, 640, "DmlExecutionProvider")
