import json
from dataclasses import asdict, is_dataclass


class EnhancedJSONEncoder(json.JSONEncoder):
    """
    A custom JSON encoder that can handle dataclasses.
    """

    def default(self, o):
        if is_dataclass(o):
            return asdict(o)
        return super().default(o)
