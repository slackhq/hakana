function apply<T>((function(\HH\Lib\Ref<T>): void) $callback, \HH\Lib\Ref<T> $ref): void {
    $callback($ref);
}

function update(): vec<string> {
    $ref = new \HH\Lib\Ref(shape('items' => vec[], 'offset' => 0));
    apply((\HH\Lib\Ref<shape('items' => vec<string>, 'offset' => int)> $state) ==> {
        $state->value['items'][] = 'item';
        $state->value['offset'] = 1;
    }, $ref);
    return $ref->value['items'];
}
