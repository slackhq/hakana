namespace N {
    final class ThisIsUsed {
        const keyset<classname<mixed>> CLASSES = keyset[
            nameof UsedViaConst
        ];
    }

    final class ThisIsStillUsed {}

    final class ThisIsUnused {}

    final class UsedViaConst {}

    function foo(): void {
        echo nameof ThisIsStillUsed;
        echo nameof \ThisIsUsedAsWell;
    }
}

namespace {
    final class ThisIsAlsoUsed {}

    final class ThisIsUsedAsWell {}

    final class ThisIsAlsoUnused {}

    <<__EntryPoint>>
    function main(): void {
        echo nameof N\ThisIsUsed;
        echo nameof ThisIsAlsoUsed;

        N\foo();
    }
}