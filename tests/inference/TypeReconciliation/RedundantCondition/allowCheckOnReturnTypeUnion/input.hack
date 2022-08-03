function returnsInt(): arraykey {
    return rand(0, 1) ? 1 : "hello";
}

if (is_int(returnsInt())) {}
if (!is_int(returnsInt())) {}