"""This file contains the hostcalls that are used by the SPEAR runtime 
to interact with the host system."""

from dataclasses import dataclass
from enum import IntEnum
from typing import Any

from dataclasses_json import dataclass_json



class TransformType(IntEnum):
    """
    The type of the data that is being transformed.
    """

    IMAGE = 0
    TEXT = 1
    AUDIO = 2
    VIDEO = 3
    TENSOR = 4
    UNKNOWN = 5


class TransformOperation(IntEnum):
    """
    The operation that is being performed on the data.
    """

    LLM = 0
    OCR = 1
    TEXT_TO_SPEECH = 2
    SPEECH_TO_TEXT = 3
    TEXT_TO_IMAGE = 4


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
class ChatMessage:
    """
    The message object for the chat hostcall.
    """

    role: str
    content: str


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
class TransformResponse:
    """
    The response object for the transform hostcall.
    """

    model: str
    id: str
    choices: list[ChatChoice]
