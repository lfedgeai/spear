""" This file contains the relevant dataclasses and types for tools related hostcalls.
"""

from dataclasses_json import dataclass_json
from dataclasses import dataclass

@dataclass_json
@dataclass
class NewToolParams:
    """
    The parameters for the newtool hostcall.
    """
    name: str
    type: str
    description: str
    required: bool

@dataclass_json
@dataclass
class NewToolRequest:
    """
    The request object for the newtool hostcall.
    """
    name: str
    description: str
    params: list[NewToolParams]
    cb: str

@dataclass_json
@dataclass
class NewToolResponse:
    """
    The response object for the newtool hostcall.
    """
    tool_id: str

@dataclass_json
@dataclass
class NewToolsetRequest:
    """
    The request object for the newtoolset hostcall.
    """
    name: str
    description: str
    tool_ids: list[str]


@dataclass_json
@dataclass
class NewToolsetResponse:
    """
    The response object for the newtoolset hostcall.
    """
    toolset_id: str

@dataclass_json
@dataclass
class ToolsetInstallBuiltinsRequest:
    """
    The request object for the toolset.install_builtins hostcall.
    """
    toolset_id: str

@dataclass_json
@dataclass
class ToolsetInstallBuiltinsResponse:
    """
    The response object for the toolset.install_builtins hostcall.
    """
    toolset_id: str
