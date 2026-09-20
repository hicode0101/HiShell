#!/usr/bin/env python3
"""
最小 SSH 测试服务器（仅本地端到端测试用）
- 监听 127.0.0.1:2222
- 密码认证：root / root123
- 登录后发送横幅 + 提示符，回显输入的命令（echo 式）
用法：python test/ssh_server.py
"""
import socket
import sys
import threading

import paramiko

HOST_KEY = paramiko.RSAKey.from_private_key_file(__file__.replace("ssh_server.py", "test_rsa_key"))
HOST, PORT = "127.0.0.1", 2222
USER, PASSWORD = "root", "root123"


class ServerInterface(paramiko.ServerInterface):
    def check_channel_request(self, kind, chanid):
        if kind == "session":
            return paramiko.OPEN_SUCCEEDED
        return paramiko.OPEN_FAILED_ADMINISTRATIVELY_PROHIBITED

    def check_auth_password(self, username, password):
        if username == USER and password == PASSWORD:
            return paramiko.AUTH_SUCCESSFUL
        return paramiko.AUTH_FAILED

    def check_auth_publickey(self, username, key):
        return paramiko.AUTH_FAILED

    def get_allowed_auths(self, username):
        return "password,keyboard-interactive"

    def check_auth_interactive(self, username, submethods):
        return paramiko.AUTH_FAILED

    def check_channel_pty_request(self, channel, term, width, height, pixelwidth, pixelheight, modes):
        return True

    def check_channel_shell_request(self, channel):
        return True


def handle_client(sock, addr):
    try:
        transport = paramiko.Transport(sock)
        transport.add_server_key(HOST_KEY)
        transport.start_server(server=ServerInterface())
        chan = transport.accept(20)
        if chan is None:
            return
        banner = "Welcome to HiShell test sshd\r\n"
        chan.send(banner)
        chan.send("test$ ")
        buf = b""
        while True:
            data = chan.recv(4096)
            if not data:
                break
            buf += data
            while b"\r" in buf or b"\n" in buf:
                idx = min(
                    buf.index(b"\r") if b"\r" in buf else len(buf),
                    buf.index(b"\n") if b"\n" in buf else len(buf),
                )
                line = buf[:idx].decode(errors="replace")
                buf = buf[idx + 1:]
                if line.strip() == "exit":
                    chan.send("bye\r\n")
                    chan.close()
                    transport.close()
                    return
                if line.strip():
                    chan.send(f"{line}\r\nOK-{line.strip().split()[0]}\r\ntest$ ")
        chan.close()
        transport.close()
    except Exception as e:  # noqa
        print(f"[ssh_server] client {addr} error: {e}", file=sys.stderr)


def main():
    server = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    server.bind((HOST, PORT))
    server.listen(10)
    print(f"[ssh_server] listening on {HOST}:{PORT} (user={USER} password={PASSWORD})", flush=True)
    while True:
        sock, addr = server.accept()
        threading.Thread(target=handle_client, args=(sock, addr), daemon=True).start()


if __name__ == "__main__":
    main()
