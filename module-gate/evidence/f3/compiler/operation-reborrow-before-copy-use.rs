
#[allow(dead_code)]
fn f3_borrow_probe<W: WriterNamespace>(mut scope: CatalogCallScope<'_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, '_, W>, private: &mut [u8;425_984], object: ObjectId, cursor: &mut Plan4ResidencyCursor) {
let step = lookup(scope.operation(private).unwrap(), object, cursor).unwrap();
if let CatalogLookupStep::Found(record) = step { let copied = record;
let _again=scope.operation(private); std::hint::black_box(copied.header());
} else { scope.finish_checked().unwrap(); }
}
