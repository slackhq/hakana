use namespace HH\Lib\Vec;

function identity<T>(T $value): T {
    return $value;
}

function lookup(dict<string, int> $values, string $key): void {
    $value = identity(idx($values, $key));
    if ($value === null) {
        print('missing');
    }
}

function concat(vec<string> $values): void {
    $items = Vec\concat(
        vec[shape('value' => '', 'flag' => true)],
        Vec\map($values, $value ==> shape('value' => $value, 'flag' => false)),
    );
    foreach ($items as $item) {
        if ($item['flag']) {
            print($item['value']);
        }
    }
}
