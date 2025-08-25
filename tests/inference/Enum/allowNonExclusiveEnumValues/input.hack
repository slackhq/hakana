enum Category: string {
    BUSINESS = "business";
    PERSONAL = "personal";
    SHARED = "shared";
}

abstract class AbstractDocument {
    <<Hakana\AllowNonExclusiveEnumValues>>
    abstract const Category DOCUMENT_CATEGORY;
}

final class ContractDocument extends AbstractDocument {
    const Category DOCUMENT_CATEGORY = Category::BUSINESS;
}

final class InvoiceDocument extends AbstractDocument {
    const Category DOCUMENT_CATEGORY = Category::BUSINESS;  // OK - same enum value allowed
}

final class PersonalDocument extends AbstractDocument {
    const Category DOCUMENT_CATEGORY = Category::PERSONAL;
}