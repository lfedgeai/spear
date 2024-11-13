"""This file contains the hostcalls that are used by the SPEAR runtime 
to interact with the host system."""

from enum import IntEnum

class TransformType(IntEnum):
    IMAGE = 0
    TEXT = 1
    AUDIO = 2
    VIDEO = 3
    TENSOR = 4
    UNKNOWN = 5

class TransformOperation(IntEnum):
    LLM = 0
    OCR = 1
    TEXT_TO_SPEECH = 2
    SPEECH_TO_TEXT = 3
    TEXT_TO_IMAGE = 4