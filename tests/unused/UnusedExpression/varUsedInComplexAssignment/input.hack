function bytePairMerge(string $piece): vec<shape('start' => int, 'rank' => int)> {
    // This is a vector of (start, rank).
    // The rank is of the pair starting at position start.
    $parts = vec[];
    $piece_len = Str\length($piece);

    if ($piece_len < 2) {
        return vec[
            shape('start' => 0, 'rank' => \PHP_INT_MAX),
            shape('start' => $piece_len, 'rank' => \PHP_INT_MAX),
        ];
    }

    // Note that we hash bytes when indexing into ranks, not token pairs
    $min_rank = shape('rank' => \PHP_INT_MAX, 'pos' => \PHP_INT_MAX);

    for ($i = 0; $i < $piece_len - 1; $i++) {
        $rank = rand(0, 1000);
        if ($rank < $min_rank['rank']) {
            $min_rank = shape('rank' => $rank, 'pos' => $i);
        }
        $parts[] = shape('start' => $i, 'rank' => $rank);
    }
    $parts[] = shape('start' => $piece_len - 1, 'rank' => \PHP_INT_MAX);
    $parts[] = shape('start' => $piece_len, 'rank' => \PHP_INT_MAX);

    // If you have n parts and m merges, this does O(mn) work.
    while ($min_rank['rank'] !== \PHP_INT_MAX) {
        $i = $min_rank['pos'];

        // Update parts[i] and parts[i - 1] before removing parts[i + 1]
        if ($i > 0) {
            $parts[$i - 1] = shape(
                'start' => $parts[$i - 1]['start'],
                'rank' => rand(0, 100),
            );
        }
        $parts[$i] = shape(
            'start' => $parts[$i]['start'],
            'rank' => rand(0, 100),
        );

        // Remove parts[i + 1]
        $parts = Vec\concat(Vec\take($parts, $i + 1), Vec\drop($parts, $i + 2));

        // Find next minimum rank
        $min_rank = shape('rank' => \PHP_INT_MAX, 'pos' => \PHP_INT_MAX);
        for ($j = 0; $j < C\count($parts) - 1; $j++) {
            if ($parts[$j]['rank'] < $min_rank['rank']) {
                $min_rank = shape('rank' => $parts[$j]['rank'], 'pos' => $j);
            }
        }
    }

    return $parts;
}
