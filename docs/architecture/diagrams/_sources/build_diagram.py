#!/usr/bin/env python3
"""Build a drawio mxfile XML from a structured spec.

Reads the generic catalog (hand-mirrored below from
thurbeen-skills/drawio/templates/stencils.yaml) and the mxfile
template from the skill's template dir, then renders nodes + edges
into the template.

Usage:
    build_diagram.py <spec.json> <output.xml>

Spec format documented in this directory's specs (system-context,
deployment-topology, iac-pipeline, supply-chain).

Cell ids start at 10 (0 and 1 are sentinels reserved by drawio).
"""
from __future__ import annotations

import json
import sys
from html import escape
from pathlib import Path

SKILL_DIR = Path("/home/magicletur/Repositories/thurbeen-skills/skills/drawio")
TEMPLATE_PATH = SKILL_DIR / "templates" / "mxfile.xml.tmpl"

# Subset of the generic catalog used by the four thurward diagrams.
# Source of truth: thurbeen-skills/skills/drawio/templates/stencils.yaml
# under the `generic:` key.
#
# IMPORTANT: drawio 29.6.1 headless export (`-x -f svg`) silently drops
# the `mxgraph.networking.*` and `mxgraph.rack.*` stencils — they render
# as empty rectangles. The `mxgraph.cisco.*` library (older but bundled)
# DOES render in headless mode and provides usable substitutes for
# firewall/router/switch/server. We override the central catalog locally
# until either drawio fixes headless networking stencils or the central
# catalog is updated.
GENERIC_SERVICES = {
    "user": "shape=actor;html=1;fontSize=12;labelPosition=center;verticalLabelPosition=bottom;verticalAlign=top;align=center;outlineConnect=0;",
    "pc": "shape=mxgraph.cisco.computers_and_peripherals.pc;html=1;fontSize=12;labelPosition=center;verticalLabelPosition=bottom;verticalAlign=top;align=center;",
    "internet": "shape=cloud;whiteSpace=wrap;html=1;fontSize=12;fillColor=#dae8fc;strokeColor=#6c8ebf;",
    "cloud": "shape=cloud;whiteSpace=wrap;html=1;fontSize=12;fillColor=#f5f5f5;strokeColor=#666666;",
    "dns": "shape=hexagon;perimeter=hexagonPerimeter2;whiteSpace=wrap;html=1;fontSize=12;fillColor=#d5e8d4;strokeColor=#82b366;",
    "load_balancer": "shape=mxgraph.networking.lb;html=1;labelPosition=center;verticalLabelPosition=bottom;verticalAlign=top;align=center;fillColor=#000000;strokeColor=#000000;",
    "api_gateway": "shape=hexagon;perimeter=hexagonPerimeter2;whiteSpace=wrap;html=1;fontSize=12;fillColor=#dae8fc;strokeColor=#6c8ebf;",
    "firewall": "shape=mxgraph.cisco.security.firewall;html=1;fontSize=12;labelPosition=center;verticalLabelPosition=bottom;verticalAlign=top;align=center;",
    "router": "shape=mxgraph.cisco.routers.router;html=1;fontSize=12;labelPosition=center;verticalLabelPosition=bottom;verticalAlign=top;align=center;",
    "switch": "shape=mxgraph.cisco.switches.workgroup_switch;html=1;fontSize=12;labelPosition=center;verticalLabelPosition=bottom;verticalAlign=top;align=center;",
    "bridge": "shape=mxgraph.cisco.misc.bridge;html=1;fontSize=12;labelPosition=center;verticalLabelPosition=bottom;verticalAlign=top;align=center;",
    "server": "shape=mxgraph.cisco.servers.standard_host;html=1;fontSize=12;labelPosition=center;verticalLabelPosition=bottom;verticalAlign=top;align=center;",
    "vm": "shape=cube;whiteSpace=wrap;html=1;darkOpacity=0.05;fontSize=12;fillColor=#dae8fc;strokeColor=#6c8ebf;",
    "container": "rounded=1;whiteSpace=wrap;html=1;fillColor=#dae8fc;strokeColor=#6c8ebf;fontSize=12;",
    "function": "shape=mxgraph.flowchart.predefined_process;whiteSpace=wrap;html=1;fontSize=12;fillColor=#fff2cc;strokeColor=#d6b656;",
    "web_app": "rounded=1;whiteSpace=wrap;html=1;fillColor=#d5e8d4;strokeColor=#82b366;fontSize=12;",
    "database": "shape=cylinder3;whiteSpace=wrap;html=1;boundedLbl=1;backgroundOutline=1;size=15;fontSize=12;fillColor=#dae8fc;strokeColor=#6c8ebf;",
    "cache": "shape=cylinder3;whiteSpace=wrap;html=1;boundedLbl=1;backgroundOutline=1;size=15;fontSize=12;fillColor=#ffe6cc;strokeColor=#d79b00;",
    "storage_bucket": "shape=mxgraph.basic.bucket;whiteSpace=wrap;html=1;fontSize=12;fillColor=#d5e8d4;strokeColor=#82b366;",
    "blob_object": "shape=note;whiteSpace=wrap;html=1;backgroundOutline=1;darkOpacity=0.05;fontSize=12;fillColor=#f5f5f5;strokeColor=#666666;",
    "monitor": "shape=mxgraph.flowchart.display;whiteSpace=wrap;html=1;fontSize=12;fillColor=#dae8fc;strokeColor=#6c8ebf;",
    "log_store": "shape=document;whiteSpace=wrap;html=1;boundedLbl=1;fontSize=12;fillColor=#f5f5f5;strokeColor=#666666;",
    "identity_provider": "shape=hexagon;perimeter=hexagonPerimeter2;whiteSpace=wrap;html=1;fontSize=12;fillColor=#f8cecc;strokeColor=#b85450;",
    "secret_store": "shape=mxgraph.flowchart.stored_data;whiteSpace=wrap;html=1;fontSize=12;fillColor=#f8cecc;strokeColor=#b85450;",
}

ZONE_STYLES = {
    "soft_gray":  "rounded=0;whiteSpace=wrap;html=1;dashed=1;fillColor=#E6E6E6;strokeColor=none;labelBackgroundColor=none;",
    "wireframe":  "rounded=0;whiteSpace=wrap;html=1;dashed=1;fillColor=none;strokeColor=#7F7F7F;labelBackgroundColor=none;",
    "inner_gray": "rounded=0;whiteSpace=wrap;html=1;dashed=1;fillColor=#F0F0F0;strokeColor=none;labelBackgroundColor=none;",
}

TITLE_STYLE = "text;html=1;strokeColor=none;fillColor=none;align=center;verticalAlign=middle;whiteSpace=wrap;rounded=0;fontSize=14;fontStyle=1;"

EDGE_BASE = "edgeStyle=orthogonalEdgeStyle;rounded=0;html=1;jettySize=auto;orthogonalLoop=1;jumpStyle=arc;jumpSize=8;labelBackgroundColor=none;"
EDGE_DATA = EDGE_BASE + "fillColor=#dae8fc;strokeColor=#6c8ebf;"
EDGE_CONTROL = EDGE_BASE + "dashed=1;"
EDGE_PEERING = EDGE_BASE + "startArrow=classic;endArrow=classic;strokeColor=#6c8ebf;fillColor=#dae8fc;"


def style_for_key(key: str) -> str:
    if not key.startswith("generic."):
        raise ValueError(f"only generic.* keys supported; got {key!r}")
    name = key[len("generic."):]
    if name not in GENERIC_SERVICES:
        raise KeyError(f"unknown stencil: generic.{name}")
    return GENERIC_SERVICES[name]


def edge_style(edge: dict) -> str:
    if edge.get("bidirectional"):
        return EDGE_PEERING
    kind = edge.get("kind", "data")
    return {"data": EDGE_DATA, "control": EDGE_CONTROL, "peering": EDGE_PEERING}[kind]


def render(spec: dict) -> str:
    page_w = spec["page"]["width"]
    page_h = spec["page"]["height"]
    cells: list[str] = []
    next_id = 10
    id_map: dict[str, int] = {}

    # Layer A: zone rectangles
    for zone in spec.get("zones", []):
        nid = next_id; next_id += 1
        id_map[zone["id"]] = nid
        zstyle = ZONE_STYLES[zone.get("fill", "soft_gray")]
        cells.append(
            f'<mxCell id="{nid}" value="" style="{zstyle}" vertex="1" parent="1">'
            f'<mxGeometry x="{zone["x"]}" y="{zone["y"]}" width="{zone["w"]}" height="{zone["h"]}" as="geometry"/>'
            f'</mxCell>'
        )
        tid = next_id; next_id += 1
        title = escape(zone["label"], quote=True)
        cells.append(
            f'<mxCell id="{tid}" value="{title}" style="{TITLE_STYLE}" vertex="1" parent="1">'
            f'<mxGeometry x="{zone["x"]}" y="{zone["y"] - 25}" width="{zone["w"]}" height="20" as="geometry"/>'
            f'</mxCell>'
        )

    # Layer C: icons
    for node in spec.get("nodes", []):
        nid = next_id; next_id += 1
        id_map[node["id"]] = nid
        style = style_for_key(node["key"])
        label = escape(node.get("label", ""), quote=True)
        w = node.get("w", 60); h = node.get("h", 60)
        cells.append(
            f'<mxCell id="{nid}" value="{label}" style="{style}" vertex="1" parent="1">'
            f'<mxGeometry x="{node["x"]}" y="{node["y"]}" width="{w}" height="{h}" as="geometry"/>'
            f'</mxCell>'
        )

    # Edges
    for edge in spec.get("edges", []):
        nid = next_id; next_id += 1
        style = edge_style(edge)
        # Optional anchor overrides: exit_x/exit_y (source side),
        # entry_x/entry_y (target side). Values are 0-1 floats along
        # the bounding box. Lets you steer where an edge attaches so
        # parallel edges don't pile up on the same anchor point.
        for axis in ("exit_x", "exit_y", "entry_x", "entry_y"):
            if axis in edge:
                style += f"{axis.replace('_', '')}={edge[axis]};"
                # exitDx/Dy default to 0; not exposing them yet
        # Optional label position along the edge (0=source, 1=target,
        # default 0.5). Useful when two edges share a corridor and you
        # want their labels at different points.
        if "label_pos" in edge:
            style += f"labelPosition=center;align=center;exitPerimeter=1;"
        label = escape(edge.get("label", ""), quote=True)
        src = id_map[edge["src"]]; dst = id_map[edge["dst"]]
        # Optional waypoints (list of {x, y}) inserted between
        # source and target anchors. drawio's orthogonal router will
        # bend at these points instead of guessing.
        geom_body = ""
        if "waypoints" in edge:
            pts = "".join(
                f'<mxPoint x="{p["x"]}" y="{p["y"]}"/>'
                for p in edge["waypoints"]
            )
            geom_body = f'<Array as="points">{pts}</Array>'
        # Optional x-offset for the label along the edge (relative).
        # -1..1 range; 0 = midpoint.
        if "label_x" in edge:
            geom_body += f'<mxPoint x="{edge["label_x"]}" as="offset"/>'
        cells.append(
            f'<mxCell id="{nid}" value="{label}" style="{style}" edge="1" parent="1" '
            f'source="{src}" target="{dst}">'
            f'<mxGeometry relative="1" as="geometry">{geom_body}</mxGeometry>'
            f'</mxCell>'
        )

    tmpl = TEMPLATE_PATH.read_text()
    return (
        tmpl
        .replace("{{DX}}", str(page_w + 200))
        .replace("{{DY}}", str(page_h + 200))
        .replace("{{PAGE_WIDTH}}", str(page_w))
        .replace("{{PAGE_HEIGHT}}", str(page_h))
        .replace("{{CELLS}}", "\n".join(cells))
    )


def main() -> int:
    if len(sys.argv) != 3:
        print("usage: build_diagram.py <spec.json> <output.xml>", file=sys.stderr)
        return 2
    spec = json.loads(Path(sys.argv[1]).read_text())
    Path(sys.argv[2]).write_text(render(spec))
    print(sys.argv[2])
    return 0


if __name__ == "__main__":
    sys.exit(main())
