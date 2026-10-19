function union_to_keys(vec<string> $first, vec<string> $second): vec<string> {
    $combined = Keyset\union($first, $second);
    return Vec\keys($combined);
}

function literal_union(string $value, keyset<string> $existing): keyset<string> {
    return Keyset\union($existing, keyset[$value]);
}

function nested_literal_union(shape('id' => string) $item): keyset<string> {
    return Keyset\union(keyset[], keyset[$item['id']]);
}

function intersection_to_keys(vec<string> $values, keyset<string> $allowed): vec<string> {
    $supported = $values;
    return Keyset\intersect($supported, $allowed) |> Vec\keys($$);
}

function keyset_cast_pipe(vec<string> $values): vec<string> {
    return $values |> keyset($$) |> Vec\keys($$);
}

function appended_keyset(vec<string> $values): vec<string> {
    $collected = keyset[];
    foreach ($values as $value) {
        $collected[] = $value;
    }
    return Vec\keys($collected);
}

function mapped_keyset(vec<string> $values): keyset<string> {
    $mapped = Keyset\map($values, $value ==> Str\uppercase($value));
    return Keyset\keys($mapped);
}

function filtered_keyset(vec<string> $values): keyset<string> {
    $filtered = Keyset\filter($values, $value ==> $value !== '');
    return Keyset\keys($filtered);
}

function chunked_keyset(vec<string> $values): vec<string> {
    $chunks = Keyset\chunk($values, 2);
    return Vec\keys($chunks[0]);
}

function flattened_keyset(vec<vec<string>> $values): keyset<string> {
    $flattened = Keyset\flatten($values);
    return Keyset\keys($flattened);
}

function sliced_keyset(vec<string> $values): vec<string> {
    return Keyset\drop(Keyset\take($values, 3), 1) |> Vec\keys($$);
}

function difference_to_keys(vec<string> $values, vec<string> $excluded): keyset<string> {
    $remaining = Keyset\diff($values, $excluded);
    return Keyset\keys($remaining);
}

function literal_chunk(string $first, string $second): vec<string> {
    $chunks = Keyset\chunk(vec[$first, $second], 2);
    return Vec\keys($chunks[0]);
}

function map_with_keys(dict<string, string> $values): keyset<string> {
    $mapped = Keyset\map_with_key($values, ($key, $value) ==> $key.$value);
    return Keyset\keys($mapped);
}

function nonnull_keys(vec<?string> $values): keyset<string> {
    $filtered = Keyset\filter_nulls($values);
    return Keyset\keys($filtered);
}

async function mapped_async_keys(vec<string> $values): Awaitable<vec<string>> {
    $mapped = await Keyset\map_async($values, async $value ==> Str\uppercase($value));
    return Vec\keys($mapped);
}

async function filtered_async_keys(vec<string> $values): Awaitable<vec<string>> {
    $filtered = await Keyset\filter_async($values, async $value ==> $value !== '');
    return Vec\keys($filtered);
}
