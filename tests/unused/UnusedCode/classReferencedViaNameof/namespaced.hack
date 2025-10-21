namespace N;

use namespace Imported\SomeNamespace;

final class ThisIsUsed {
    const keyset<classname<mixed>> CLASSES = keyset[
        nameof UsedViaConst,
        nameof SomeNamespace\UsedViaNamespaceUse,
    ];
}

final class ThisIsStillUsed {}

final class ThisIsUnused {}

final class UsedViaConst {}

function foo(): void {
    echo nameof ThisIsStillUsed;
    echo nameof \ThisIsUsedAsWell;
}
