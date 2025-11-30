function helper(int $x): int {
    return $x * 2;
}

function process(vec<int> $items): vec<int> {
    $results = vec[];
    foreach ($items as $item) {
        $results[] = helper($item);
    }
    return $results;
}

function main(): void {
    $data = vec[1, 2, 3];
    $processed = process($data);
    $single = helper(5);
}
