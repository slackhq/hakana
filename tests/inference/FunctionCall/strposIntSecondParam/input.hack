function hasZeroByteOffset(string $s) : bool {
    return strpos($s, 0) !== false;
}