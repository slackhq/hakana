use namespace HH\Lib\C;

function empty_tuple(): int {
    $result = new HH\Lib\Ref(tuple(vec[], keyset[]));
    $f = () ==> {
        $result->value = tuple(vec[1], keyset['a']);
    };
    $f();
    list($rows, $failed) = $result->value;
    return C\count($rows) + C\count($failed);
}

function null_until_called(): int {
    $passed = new HH\Lib\Ref(null);
    $f = (int $a, string $b) ==> {
        $passed->value = tuple($a, $b);
    };
    $f(1, 'a');
    list($a, $_b) = $passed->value as nonnull;
    return $a;
}

async function returned_from_closure(): Awaitable<int> {
    $capture = async (): Awaitable<(int, bool)> ==> {
        $seen = new HH\Lib\Ref(null);
        $f = () ==> {
            $seen->value = tuple(1, true);
        };
        $f();
        return $seen->value as nonnull;
    };
    list($id, $_) = await $capture();
    return $id;
}

function never_a_tuple(): void {
    $ref = new HH\Lib\Ref(5);
    list($a, $_b) = $ref->value;
    echo $a;
}

function takes_int(int $i): void {}

async function slot_filled_later(): Awaitable<void> {
    $run = async () ==> {
        $res = new HH\Lib\Ref(null);
        $f = () ==> {
            $res->value = 1;
        };
        $f();
        return tuple($res->value as nonnull, 'a');
    };
    list($res, $_) = await $run();
    takes_int($res);
}
