// This should error - interface with only one implementor
interface A {}

abstract class B implements A {}

// This should also error - interface with only one concrete implementor
interface C {}

final class D implements C {}

// This is fine - interface with multiple implementors
interface E {}

abstract class F implements E {}

abstract class G implements E {}

// This is fine - interface with no implementors
interface H {}
