#ifndef TOPO_CHECK_RUSTCALLEDGEEXTRACTOR_H
#define TOPO_CHECK_RUSTCALLEDGEEXTRACTOR_H

#include "topo/Check/CallEdgeExtractor.h"

#include <string>
#include <vector>

namespace topo::check {

/// L1 regex-based Rust call edge extractor used by StageIsolationCheck and
/// VisibilityCheck. Scans function bodies for `identifier(...)` and
/// `path::identifier(...)` calls and emits caller→callee edges qualified by
/// the enclosing mod/impl/fn scope (mirrors RustCallSiteExtractor's
/// scope-tracking state machine).
class RustCallEdgeExtractor : public CallEdgeExtractor {
public:
    std::vector<CallEdge> extractCallEdges(const std::string& filePath) override;
};

} // namespace topo::check

#endif // TOPO_CHECK_RUSTCALLEDGEEXTRACTOR_H
