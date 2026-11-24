newtype channel_id_t as int = int;

enum Level: int as int {
    LOW = 1;
    HIGH = 2;
}

function remove_from_keyset(keyset<string> $names, string $name): keyset<string> {
    unset($names[$name]);
    return $names;
}

function read_keyset(keyset<string> $names, string $name): string {
    return $names[$name];
}

function read_newtype_key(dict<channel_id_t, string> $names, int $id): string {
    return $names[$id];
}

function read_enum_key(int $level): string {
    return Level::getNames()[$level];
}

function read_narrower_key(dict<Level, string> $names, arraykey $key): string {
    return $names[$key];
}

function read_literal_keys(int $i): string {
    $names = dict[1 => 'a', 2 => 'b'];
    return $names[$i];
}

function read_template_key<T as arraykey>(dict<int, string> $names, T $key): string {
    return $names[$key];
}

function bad_keyset_offset(keyset<string> $names): string {
    return $names[0];
}

function bad_dict_offset(dict<channel_id_t, string> $names, string $id): string {
    return $names[$id];
}

function bad_vec_offset(vec<string> $names): string {
    return $names['a'];
}
