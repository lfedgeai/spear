#!/usr/bin/env python3
import sys

import spear.client as client
import spear.transform.chat as chat
from spear.utils.tool import register_internal_tool

from spear.proto.tool import BuiltinToolID

# How to run this file:
# compile the code and execute it with spearlet binary.
# make sure you also have installed the compiled spear python package.
# command to install the spear python package:
#   make build; pip install sdk/python/dist/spear-0.0.1-py3-none-any.whl --force-reinstall
# command to execute this demo:
#   spearlet -L <spear_root>/demo/codelab/ exec -t process -f codelab.py


agent = client.HostAgent()


def test_builtin_tool():
    """
    test builtin tool
    """
    resp = chat.chat(agent,
                     "help me to open sjsu's homepage",
                     model="gpt-4o", builtin_tools=[
                         BuiltinToolID.BuiltinToolID.OpenURL,
                     ])
    print(resp, file=sys.stderr)


def test_tool_cb(content):
    """
    spear tool function for showing a html page

    @param content: html content string
    """
    import tempfile
    import webbrowser
    with tempfile.NamedTemporaryFile(mode='w', suffix='.html', delete=False) as temp_file:
        # Write the HTML content to the temporary file
        temp_file.write(content)
        # Get the file path of the temporary file
        temp_file_path = temp_file.name
    webbrowser.open(f'file://{temp_file_path}')

    return "done displaying html content"


def test_internal_tool():
    """
    test internal tool
    """
    tid = register_internal_tool(agent, test_tool_cb)
    resp = chat.chat(agent,
                     "help me to display a html page with one button to say hello",
                     model="gpt-4o", internal_tools=[
                         tid,
                     ])
    print(resp, file=sys.stderr)


def handle(params):
    """
    handle the request
    """
    test_builtin_tool()
    test_internal_tool()


if __name__ == "__main__":
    agent.register_handler("handle", handle)
    agent.run()
