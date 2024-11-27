import json
import logging
import os
import queue
import selectors
import socket
import struct
import threading
import time

import spear.hostcalls as hc

RPC_TYPE_REQ = 0
RPC_TYPE_RESP_OK = 1
RPC_TYPE_RESP_ERR = 2

MAX_INFLIGHT_REQUESTS = 128


logger = logging.getLogger(__name__)


def rpc_type(obj):
    """
    determine the type of the rpc object
    """
    if "method" in obj:
        return RPC_TYPE_REQ
    elif "result" in obj:
        return RPC_TYPE_RESP_OK
    elif "error" in obj:
        return RPC_TYPE_RESP_ERR
    else:
        raise TypeError("Invalid rpc object")


class JsonRpcRequest(object):
    """
    JsonRpcRequest is the request object for the rpc call
    """

    def __init__(self, rid, method, params):
        self._method = method
        self._params = params
        self._id = rid

    def to_dict(self):
        obj = {}
        obj["method"] = self._method
        obj["params"] = self._params
        obj["id"] = self._id
        return obj

    def build_response(self, result):
        return JsonRpcOkResp(self._id, result)

    def build_error(self, code, message, data=None):
        return JsonRpcErrorResp(self._id, code, message, data)

    @property
    def method(self):
        return self._method

    @property
    def params(self):
        return self._params

    @property
    def id(self):
        return self._id


class JsonRpcOkResp(object):
    """
    JsonRpcOkResp is the response object for the successful rpc call
    """

    def __init__(self, rid, result):
        self._result = result
        self._id = rid

    def to_dict(self):
        obj = {}
        obj["result"] = self._result
        obj["id"] = self._id
        return obj

    @property
    def result(self):
        return self._result

    @property
    def id(self):
        return self._id


class JsonRpcErrorResp(object):
    """
    JsonRpcErrorResp is the response object for the failed rpc call
    """

    def __init__(self, rid, code, message, data=None):
        self._code = code
        self._message = message
        self._data = data
        self._id = rid

    def to_dict(self):
        """
        convert the object to dictionary
        """
        obj = {}
        obj["code"] = self._code
        obj["message"] = self._message
        obj["data"] = self._data
        obj["id"] = self._id
        return obj

    @property
    def code(self):
        return self._code

    @property
    def message(self):
        return self._message

    @property
    def data(self):
        return self._data

    @property
    def id(self):
        return self._id


class HostAgent(object):
    """
    HostAgent is the agent that connects to the host
    """

    _instance = None

    def __init__(self):
        self._client = None
        self._send_queue = None
        self._recv_queue = None
        self._global_id = 0
        self._send_task = None
        self._recv_task = None
        self._handlers = {}
        event_sock_r, event_sock_w = socket.socketpair()
        self._stop_event_r = event_sock_r
        self._stop_event_w = event_sock_w
        event_sock_r.setblocking(False)
        self._inflight_requests_lock = threading.Lock()
        self._inflight_requests_count = 0
        self._pending_requests = {}
        self._pending_requests_lock = threading.Lock()

    def __new__(cls):
        if cls._instance is None:
            cls._instance = super(HostAgent, cls).__new__(cls)
        return cls._instance

    def connect_host(self, host_addr, host_secret) -> socket:
        """
        create a tcp connection to the server
        """
        client = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        # convert the address to tuple
        host_addr = host_addr.split(":")
        host_addr = (host_addr[0], int(host_addr[1]))
        client.connect(host_addr)
        # send little endian secret 64-bit integer
        client.send(struct.pack("<Q", host_secret))
        self._client = client
        self._client.setblocking(False)
        self._send_queue = queue.Queue(512)
        self._recv_queue = queue.Queue(512)
        self._global_id = 0

    def run(self, host_addr, host_secret):
        """
        start the agent
        """
        logger.debug("Connecting to host %s", host_addr)
        self.connect_host(host_addr, host_secret)

        logger.debug("Starting I/O threads")
        # start the send thread
        send_thread = threading.Thread(target=self._send_thread)
        send_thread.start()
        self._send_task = send_thread

        # start the recv thread
        recv_thread = threading.Thread(target=self._recv_thread)
        recv_thread.start()
        self._recv_task = recv_thread

        self._main_loop()

    def _main_loop(self):
        """
        main loop to handle the rpc calls
        """

        def handle_worker(handler, rpc_data):
            with self._inflight_requests_lock:
                self._inflight_requests_count += 1
            try:
                result = handler(rpc_data.params)
                self._put_rpc_response(rpc_data.id, result)
            except Exception as e:
                self._put_rpc_error(rpc_data.id, -32603, "Internal error", str(e))
            with self._inflight_requests_lock:
                self._inflight_requests_count -= 1
            logger.debug("Inflight requests: %d", self._inflight_requests_count)

        while True:
            rpc_data = self._get_rpc_data()
            if isinstance(rpc_data, JsonRpcRequest):
                handler = self._handlers.get(rpc_data.method)
                if handler is None:
                    logger.error("Method not found: %s", rpc_data.method)
                    self._put_rpc_error(
                        rpc_data.id, -32601, "Method not found", "Method not found"
                    )
                else:
                    if self._inflight_requests_count > MAX_INFLIGHT_REQUESTS:
                        self._put_rpc_error(
                            rpc_data.id,
                            -32000,
                            "Too many requests",
                            "Too many requests",
                        )
                    else:
                        # create a thread to handle the request
                        threading.Thread(
                            target=handle_worker, args=(handler, rpc_data)
                        ).start()
            elif isinstance(rpc_data, JsonRpcOkResp) or isinstance(
                rpc_data, JsonRpcErrorResp
            ):
                with self._pending_requests_lock:
                    req = self._pending_requests.get(rpc_data.id)
                    if req is None:
                        logger.error("Invalid response id: %d", rpc_data.id)
                    else:
                        req["cb"](rpc_data)
                        del self._pending_requests[rpc_data.id]
            else:
                logger.error("Invalid rpc data")

    def register_handler(self, method, handler):
        """
        register the handler for the method
        """
        self._handlers[method] = handler

    def unregister_handler(self, method):
        """
        unregister the handler for the method
        """
        del self._handlers[method]

    def _put_raw_object(self, obj):
        """
        finalize the data and add it to the outgoing queue
        """
        logger.debug("Putting raw data to queue: %s", str(obj))
        json_data = json.dumps(obj, ensure_ascii=False, cls=hc.EnhancedJSONEncoder)
        self._send_queue.put(json_data)

    def _get_raw_object(self):
        """
        get the object from the incoming queue
        """
        obj = self._recv_queue.get()
        return obj

    def _get_rpc_data(self):
        obj = self._get_raw_object()
        if "jsonrpc" not in obj:
            raise TypeError("Invalid jsonrpc version")
        if obj["jsonrpc"] != "2.0":
            raise TypeError("Invalid jsonrpc version")
        if "id" not in obj:
            raise TypeError("Invalid jsonrpc id")
        rtype = rpc_type(obj)
        if rtype == RPC_TYPE_REQ:
            return JsonRpcRequest(obj["id"], obj["method"], obj["params"])
        elif rtype == RPC_TYPE_RESP_OK:
            return JsonRpcOkResp(obj["id"], obj["result"])
        elif rtype == RPC_TYPE_RESP_ERR:
            return JsonRpcErrorResp(
                obj["id"],
                obj["error"]["code"],
                obj["error"]["message"],
                obj["error"]["data"],
            )
        else:
            raise TypeError("Invalid rpc object")

    def exec_request(self, method, param):
        """
        send the rpc request and return the response
        """
        # create mutex
        mutex = threading.Lock()
        # create a condition variable
        cond = threading.Condition(mutex)
        # create a list to store the response
        response = []

        def cb(rpc_data):
            with mutex:
                response.append(rpc_data)
                cond.notify()

        self._put_rpc_request(method, param, cb)
        with mutex:
            cond.wait()
            return response[0]

    def _put_rpc_request(self, method, param, cb):
        obj = {}
        obj["id"] = self._global_id
        self._global_id += 1
        obj["jsonrpc"] = "2.0"
        obj["method"] = method
        obj["params"] = param
        self._put_raw_object(obj)
        with self._pending_requests_lock:
            self._pending_requests[obj["id"]] = {
                "time": time.time(),
                "obj": obj,
                "cb": cb,
            }

    def _put_rpc_response(self, req_id, result):
        obj = {}
        obj["id"] = req_id
        obj["jsonrpc"] = "2.0"
        obj["result"] = result
        self._put_raw_object(obj)

    def _put_rpc_error(self, req_id, code, message, data=None):
        obj = {}
        obj["id"] = req_id
        obj["jsonrpc"] = "2.0"
        obj["error"] = {}
        obj["error"]["code"] = code
        obj["error"]["message"] = message
        obj["error"]["data"] = data
        self._put_raw_object(obj)

    def _send_thread(self):
        """
        send the data to the socket
        """

        def send_data():
            while not self._send_queue.empty():
                strdata = self._send_queue.get()
                data = strdata.encode("utf-8")
                # get the length of utf8 string
                length = len(data)
                lendata = length.to_bytes(8, byteorder="little")
                # send the length of the data
                self._client.sendall(lendata)
                logger.info("Sending Data: %s", data)
                self._client.sendall(data)

        sel = selectors.DefaultSelector()
        sel.register(self._stop_event_r, selectors.EVENT_READ)
        sel.register(self._client, selectors.EVENT_WRITE)
        while True:
            events = sel.select(timeout=None)
            for key, _ in events:
                if key.fileobj == self._stop_event_r:
                    # send remaining data
                    send_data()
                    return
                if key.fileobj == self._client:
                    send_data()

    def _recv_thread(self):
        """
        get the data from socket and parse it
        """

        def recv_data():
            # read int64 from the socket and convert to integer
            data = self._client.recv(8)
            length = int.from_bytes(data, byteorder="little")
            logger.info("Received Length: %d", length)
            # read the json data
            data = b""
            while len(data) == 0:
                try:
                    data = self._client.recv(length)
                except BlockingIOError as e:
                    if e.errno == 11:
                        continue                 
            logger.info("Received Data: %s", data)
            obj = json.loads(data.decode("utf-8"))
            self._recv_queue.put(obj)

        sel = selectors.DefaultSelector()
        sel.register(self._client, selectors.EVENT_READ)
        sel.register(self._stop_event_r, selectors.EVENT_READ)
        while True:
            events = sel.select(timeout=None)
            for key, _ in events:
                if key.fileobj == self._stop_event_r:
                    return
                if key.fileobj == self._client:
                    recv_data()

    def stop(self):
        """
        stop the agent
        """

        def stop_worker():
            # wait until all the inflight requests are completed
            while True:
                with self._inflight_requests_lock:
                    if self._inflight_requests_count == 0:
                        break
            self._stop_event_w.send(b"\x01")
            self._send_task.join()
            self._recv_task.join()
            logger.debug("Stopping the agent")
            self._client.close()
            os._exit(0)

        # create a thread to stop the agent
        threading.Thread(target=stop_worker).start()
