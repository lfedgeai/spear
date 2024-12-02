"""This file contains the hostcalls that are used by the SPEAR runtime 
to interact with the host system."""

from dataclasses import dataclass
from enum import IntEnum
from typing import Any

from dataclasses_json import dataclass_json
from typing import Optional



class TransformType(IntEnum):
    """
    The type of the data that is being transformed.
    """

    IMAGE = 0
    TEXT = 1
    AUDIO = 2
    VIDEO = 3
    TENSOR = 4
    VECTOR = 5
    UNKNOWN = 6


class TransformOperation(IntEnum):
    """
    The operation that is being performed on the data.
    """

    LLM = 0
    TOOLS = 1
    EMBEDDINGS = 2
    OCR = 3
    TEXT_TO_SPEECH = 4
    SPEECH_TO_TEXT = 5
    TEXT_TO_IMAGE = 6


@dataclass_json
@dataclass
class TransformRequest:
    """
    The request object for the transform hostcall.
    """

    input_types: list[TransformType]
    output_types: list[TransformType]
    operations: list[TransformOperation]
    params: Any


@dataclass_json
@dataclass
class ChatToolCallFunction:
    """
    The function object for the chat hostcall.
    """

    name: str
    arguments: str


@dataclass_json
@dataclass
class ChatToolCall:
    """
    The tool call object for the chat hostcall.
    """

    id : str
    type: str
    function: ChatToolCallFunction


@dataclass_json
@dataclass
class ChatMessage:
    """
    The message object for the chat hostcall.
    """

    role: str
    content: str
    tool_calls: Optional[list[ChatToolCall]] = None


@dataclass_json
@dataclass
class ChatChoice:
    """
    The choice object for the chat hostcall.
    """

    message: ChatMessage
    index: int
    finish_reason: str


@dataclass_json
@dataclass
class ChatResponse:
    """
    The response object for the chat hostcall.
    """

    model: str
    id: str
    choices: list[ChatChoice]


@dataclass_json
@dataclass
class TransformResponseResult:
    """
    The result object for the transform hostcall.
    """

    data: str
    type: TransformType

@dataclass_json
@dataclass
class TransformResponse:
    """
    The response object for the transform hostcall.
    """

    results: list[TransformResponseResult]

@dataclass_json
@dataclass
class ChatMessageV2ToolCallFunction:
    """
    The function object for the chat hostcall.
    """

    name: str
    arguments: str


@dataclass_json
@dataclass
class ChatMessageV2ToolCall:
    """
    The tool call object for the chat hostcall.
    """

    id: str
    type: str
    function: ChatMessageV2ToolCallFunction

@dataclass_json
@dataclass
class ChatMessageV2Metadata:
    """
    The message metadata object for the chat hostcall.
    """

    reason: Optional[str] = None
    role: Optional[str] = None
    tool_call_id: Optional[str] = None
    tool_calls: Optional[list[ChatMessageV2ToolCall]] = None

@dataclass_json
@dataclass
class ChatMessageV2:
    """
    The message object for the chat hostcall.
    """

    metadata: ChatMessageV2Metadata
    content: str

@dataclass_json
@dataclass
class ChatResponseV2:
    """
    The response object for the chat hostcall.
    """

    model: str
    id: str
    messages: list[ChatMessageV2]
