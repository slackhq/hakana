function simple(): void {
    echo "hello";
}

function with_branches(int $x): string {
    if ($x > 0) {
        if ($x > 10) {
            return "big";
        }
        return "positive";
    } else if ($x < 0) {
        return "negative";
    }
    return "zero";
}

function with_loop(vec<int> $items): int {
    $sum = 0;
    foreach ($items as $item) {
        if ($item > 0) {
            $sum += $item;
        }
    }
    return $sum;
}
