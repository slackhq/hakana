function register_setup((function(): void) $callback): void {}
function register_writer(mixed $callback): void {}
function register_test(mixed $callback): void {}
function consume<T>(T $value): void {}

function test(): void {
    $enqueued = new HH\Lib\Ref(vec[]);
    register_setup(() ==> {
        $enqueued->value = vec[];
        register_writer(async (vec<string> $task_ids) ==> {
            $enqueued->value[] = $task_ids;
        });
    });
    register_test(async () ==> {
        consume($enqueued->value);
    });
}
