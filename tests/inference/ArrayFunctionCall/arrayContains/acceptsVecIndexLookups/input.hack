function checkNames(
    vec<int> $items,
    int $i,
    int $j,
    int $k,
    int $n,
    int $key,
    int $idx,
    int $index,
    int $offset,
    int $pos,
    int $position,
    int $row_index,
    int $nextIndex,
    int $offset2,
): bool {
    return C\contains_key($items, $i) &&
        C\contains_key($items, $j) &&
        C\contains_key($items, $k) &&
        C\contains_key($items, $n) &&
        C\contains_key($items, $key) &&
        C\contains_key($items, $idx) &&
        C\contains_key($items, $index) &&
        C\contains_key($items, $offset) &&
        C\contains_key($items, $pos) &&
        C\contains_key($items, $position) &&
        C\contains_key($items, $row_index) &&
        C\contains_key($items, $nextIndex) &&
        C\contains_key($items, $offset2);
}

function checkArithmetic(vec<int> $items, int $i, int $offset): bool {
    return C\contains_key($items, 0) &&
        C\contains_key($items, -1) &&
        C\contains_key($items, $i + 1) &&
        C\contains_key($items, $i - 1) &&
        C\contains_key($items, 2 * $i + $offset);
}

final class Cursor {
    const int START_INDEX = 0;

    public function __construct(public int $index) {}

    public function getIndex(): int {
        return $this->index;
    }
}

function checkMembers(vec<int> $items, Cursor $cursor, shape('index' => int) $row): bool {
    return C\contains_key($items, $cursor->index) &&
        C\contains_key($items, $cursor->getIndex()) &&
        C\contains_key($items, $row['index']) &&
        C\contains_key($items, Cursor::START_INDEX);
}

function checkDictWithVecValues(dict<int, vec<int>> $items, int $user_id): bool {
    return C\contains_key($items, $user_id);
}

function checkKeyset(keyset<int> $items, int $user_id): bool {
    return C\contains_key($items, $user_id);
}

function checkValue(vec<int> $items, int $user_id): bool {
    return C\contains($items, $user_id);
}

function checkEmptyVecFallback(bool $empty, dict<int, bool> $map, int $user_id): bool {
    $items = $empty ? vec[] : $map;
    return C\contains_key($items, $user_id);
}

function checkStringKeyWithEmptyVecFallback(bool $empty, dict<string, bool> $map, string $name): bool {
    $items = $empty ? vec[] : $map;
    return C\contains_key($items, $name);
}

function checkUnionIndex(bool $choose_vec, vec<int> $values, dict<int, bool> $map, int $i): bool {
    $items = $choose_vec ? $values : $map;
    return C\contains_key($items, $i);
}

function checkOtherIndexNames(vec<int> $items, int $col, int $row, int $current_shard_key, int $cursor): bool {
    return C\contains_key($items, $col) &&
        C\contains_key($items, $row) &&
        C\contains_key($items, $current_shard_key) &&
        C\contains_key($items, $cursor);
}

final class Parser {
    public function __construct(private int $pointer) {}

    public function check(vec<int> $tokens): bool {
        return C\contains_key($tokens, $this->pointer);
    }
}

function checkMutableIndex(vec<int> $items): bool {
    $idx = new HH\Lib\Ref(0);
    return C\contains_key($items, $idx->value);
}

function checkAssertedIndex(vec<int> $items, mixed $index): bool {
    return C\contains_key($items, $index as int);
}
