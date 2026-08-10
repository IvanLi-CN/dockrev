from pathlib import Path
import runpy


runpy.run_path(
    str(Path(__file__).resolve().parents[1] / "generate_brand_assets.py"),
    run_name="__main__",
)
