function hasZeroByteOffset(string $s) : bool {
    return HH\Lib\Str\search($s, '0') !== false;
}