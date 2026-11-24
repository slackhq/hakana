type row_t = shape('id' => int);

function iter_query<T>(
    (function(int): KeyedContainer<int, T>) $query_fn,
    (function(T): string) $to_row,
): Generator<int, T, void> {
    foreach ($query_fn(0) as $row) {
        $to_row($row);
        yield $row;
    }
}

function empty_query(): int {
    $empty_query_fn = (int $_cursor): vec<row_t> ==> vec[];
    $count = 0;
    foreach (iter_query($empty_query_fn, $_row ==> '') as $_row) {
        $count++;
    }
    return $count;
}

async function empty_async_query(): Awaitable<int> {
    $empty_query_fn = async (int $_cursor): Awaitable<vec<row_t>> ==> vec[];
    $rows = await $empty_query_fn(0);
    $count = 0;
    foreach (iter_query($_ ==> $rows, $_row ==> '') as $key => $_row) {
        $count += $key;
    }
    return $count;
}

function empty_container(KeyedContainer<nothing, nothing> $container): int {
    $count = 0;
    foreach ($container as $_key => $_value) {
        $count++;
    }
    return $count;
}

function empty_vec(): int {
    $count = 0;
    foreach (vec[] as $_value) {
        $count++;
    }
    return $count;
}

function keeps_what_the_lambda_returns(): int {
    $f = (): vec<int> ==> vec[];
    return $f();
}

function never_assigned(): void {
    $f = (): nothing ==> {
        throw new Exception('no');
    };
    $a = $f();
    echo $a;
}
