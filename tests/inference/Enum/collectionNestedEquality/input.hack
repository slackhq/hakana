function compare_string_enum_case_to_string(keyset<string> $k, vec<string> $v, dict<string, string> $d) {
    if ($k === keyset[StringEnum::B]) {}
    if ($v === vec[StringEnum::B]) {}
    if ($d === dict['foo' => StringEnum::B]) {}
}

function compare_int_enum_case_to_string(keyset<int> $k, vec<int> $v, dict<string, int> $d) {
    if ($k === keyset[IntEnum::B]) {}
    if ($v === vec[IntEnum::B]) {}
    if ($d === dict['foo' => IntEnum::B]) {}
}

function compare_string_enum_to_string(keyset<StringEnum> $k, vec<StringEnum> $v, dict<string, StringEnum> $d) {
    if ($k === keyset['b']) {}
    if ($v === vec['b']) {}
    if ($d === dict['foo' => 'b']) {}
}

function compare_int_enum_to_int(keyset<IntEnum> $k, vec<IntEnum> $v, dict<string, IntEnum> $d) {
    if ($k === keyset[1]) {}
    if ($v === vec[1]) {}
    if ($d === dict['foo' => 1]) {}
}

enum StringEnum: string {
	B = 'b';
	C = 'c';
	D = 'd';
}

enum IntEnum: int {
	B = 1;
	C = 2;
	D = 3;
}