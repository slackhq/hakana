function get_dict(): dict<int, int> {
    return dict[0 => 1];
}

function get_sema(): \HH\Lib\Async\Semaphore<shape('a' => int), dict<int, int>> {
    return new \HH\Lib\Async\Semaphore<_, _>(
        10,
        async (shape('a' => int) $args): Awaitable<dict<int, int>> ==> {
            return get_dict();
        }
    );
}