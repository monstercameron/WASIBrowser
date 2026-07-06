// dom_agent.cpp
//
// WebKit2 injected-bundle ("web process extension") that runs inside the
// WebContent process and applies GDOM batches to the live DOM by calling
// WebCore directly. No JavaScript, no JSC.
//
// Build: compiled as part of / linked against the WinCairo WebKit tree so it can
// use WebCore internals. Loaded via the WKBundle injected-bundle SPI. The UI-side
// Go broker ships GDOM batches over the bundle message channel; this agent decodes
// them (mirror of protocol.go) and mutates the document.
//
// Threading: all DOM access happens on the web-process main thread (the bundle
// message callback already runs there). One writer per document.

#include <WebKit/WKBundle.h>
#include <WebKit/WKBundlePage.h>
#include <WebKit/WKBundleFrame.h>
#include <WebKit/WKType.h>

#include <WebCore/Document.h>
#include <WebCore/Element.h>
#include <WebCore/Text.h>
#include <WebCore/Node.h>

#include <wtf/HashMap.h>
#include <wtf/text/AtomString.h>
#include <wtf/text/WTFString.h>

#include <cstdint>
#include <cstring>

namespace gdom {

// ---- wire opcodes: keep in sync with protocol/protocol.go --------------------
enum class Op : uint16_t {
    CreateElement = 1, CreateElementNS, CreateText, SetText, SetAttr, RemoveAttr,
    SetClass, SetStyle, Append, InsertBefore, Remove, Replace, SetOuterHTML,
    SubscribeEvent, UnsubscribeEvent, Focus, Blur, SetValue, SetChecked,
    RequestMeasure, Commit,
};

// Little-endian cursor over the batch buffer.
struct Reader {
    const uint8_t* p;
    const uint8_t* end;
    bool ok = true;

    template <typename T> T read() {
        if (p + sizeof(T) > end) { ok = false; return T{}; }
        T v; std::memcpy(&v, p, sizeof(T)); p += sizeof(T); return v;
    }
    uint16_t u16() { return read<uint16_t>(); }
    uint32_t u32() { return read<uint32_t>(); }
    uint64_t u64() { return read<uint64_t>(); }
};

// Decoded string table.
struct StringTable {
    const uint32_t* offsets = nullptr;
    uint32_t count = 0;
    const char* blob = nullptr;
    uint32_t blobLen = 0;

    String at(uint32_t i) const {
        if (i >= count) return String();
        uint32_t start = offsets[i];
        uint32_t stop = (i + 1 < count) ? offsets[i + 1] : blobLen;
        if (start > stop || stop > blobLen) return String();
        return String::fromUTF8(blob + start, stop - start);
    }
};

class Agent {
public:
    explicit Agent(WebCore::Document& document) : m_document(document) { }

    // node id 0 == null; root id is injected at mount time by the broker.
    void bindRoot(uint64_t rootId, WebCore::Node& root) { m_nodes.set(rootId, &root); }

    // Apply a single GDOM batch. Returns the new revision, or 0 on revision
    // mismatch / decode error (broker re-syncs).
    uint64_t apply(const uint8_t* buf, size_t len, uint64_t currentRevision) {
        Reader r{ buf, buf + len };
        if (len < 4 || std::memcmp(buf, "GDOM", 4) != 0) return 0;
        r.p += 4;
        r.u16(); // version
        r.u16(); // flags
        r.u64(); // batchID
        uint64_t baseRevision = r.u64();
        if (baseRevision != currentRevision) return 0; // stale; broker resends

        uint32_t opCount = r.u32();
        StringTable st;
        st.count = r.u32();
        st.blobLen = r.u32();
        st.offsets = reinterpret_cast<const uint32_t*>(r.p);
        r.p += st.count * sizeof(uint32_t);
        st.blob = reinterpret_cast<const char*>(r.p);
        r.p += st.blobLen;

        for (uint32_t i = 0; i < opCount && r.ok; ++i) {
            switch (static_cast<Op>(r.u16())) {
            case Op::CreateElement: {
                uint64_t id = r.u64();
                String tag = st.at(r.u32());
                auto el = m_document.createElement(tag, /*createdByParser*/ false);
                if (!el.hasException())
                    m_nodes.set(id, el.releaseReturnValue().ptr());
                break;
            }
            case Op::CreateText: {
                uint64_t id = r.u64();
                String text = st.at(r.u32());
                auto t = WebCore::Text::create(m_document, WTFMove(text));
                m_nodes.set(id, t.ptr());
                break;
            }
            case Op::SetText: {
                auto* n = node(r.u64());
                String text = st.at(r.u32());
                if (n) n->setTextContent(WTFMove(text));
                break;
            }
            case Op::SetAttr: {
                auto* el = element(r.u64());
                String name = st.at(r.u32());
                String value = st.at(r.u32());
                if (el) el->setAttribute(AtomString(name), AtomString(value));
                break;
            }
            case Op::RemoveAttr: {
                auto* el = element(r.u64());
                String name = st.at(r.u32());
                if (el) el->removeAttribute(AtomString(name));
                break;
            }
            case Op::Append: {
                auto* parent = node(r.u64());
                auto* child = node(r.u64());
                if (parent && child) parent->appendChild(*child);
                break;
            }
            case Op::InsertBefore: {
                auto* parent = node(r.u64());
                auto* child = node(r.u64());
                auto* ref = node(r.u64());
                if (parent && child) parent->insertBefore(*child, ref);
                break;
            }
            case Op::Remove: {
                auto* n = node(r.u64());
                if (n) n->remove();
                break;
            }
            case Op::SetValue: {
                auto* el = element(r.u64());
                String value = st.at(r.u32());
                // HTMLInputElement/HTMLTextAreaElement: downcast + setValue.
                applyFormValue(el, WTFMove(value));
                break;
            }
            case Op::Commit:
                ++m_revision;
                break;
            default:
                // unhandled op: skip remaining (operand layout unknown) -> resync
                return 0;
            }
        }
        return r.ok ? m_revision : 0;
    }

private:
    WebCore::Node* node(uint64_t id) const { return id ? m_nodes.get(id) : nullptr; }
    WebCore::Element* element(uint64_t id) const {
        auto* n = node(id);
        return (n && n->isElementNode()) ? static_cast<WebCore::Element*>(n) : nullptr;
    }
    void applyFormValue(WebCore::Element*, String&&); // wired in form_values.cpp

    WebCore::Document& m_document;
    HashMap<uint64_t, WebCore::Node*> m_nodes;
    uint64_t m_revision = 0;
};

} // namespace gdom

// ---- injected-bundle entry points ------------------------------------------
// WKBundleInitialize is the symbol the WebContent process calls when it loads
// this extension. From here we register a page-load client so we can grab the
// Document and a message handler to receive GDOM batches from the Go broker.

extern "C" void WKBundleInitialize(WKBundleRef bundle, WKTypeRef /*initData*/)
{
    // TODO(wire): WKBundleSetClient -> didCreatePage to capture WKBundlePageRef,
    //             WKBundlePageSetEditorClient/loader to get the WebCore::Document,
    //             and a message handler that calls gdom::Agent::apply() with the
    //             batch bytes posted by the UI-process broker. Events flow back
    //             via WKBundlePagePostMessage as encoded EventMessages.
    (void)bundle;
}
