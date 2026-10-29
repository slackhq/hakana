function counts(vec<string> $items): dict<string, int> {
    return \HH\Lib\C\reduce(
        $items,
        ($acc, $item) ==> {
            $acc[$item] = 1 + ($acc[$item] ?? 0);
            return $acc;
        },
        dict[],
    );
}
