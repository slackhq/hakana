function foo(string $s) : string {
    @list($port, $starboard) = explode(":", $s);
    return $port;
}