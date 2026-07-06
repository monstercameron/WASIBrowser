// bundle.cpp
//
// WebKit2 injected-bundle initialization (WKBundleInitialize) for the
// GoWebBrowser WinCairo DOM agent. This file wires up the page-load and
// message-handler callbacks that connect the Go broker (UI process) to the
// gdom::Agent (web process) via the WebKit2 bundle message channel.
//
// Threading model: all DOM access and bundle message callbacks run on the
// web-process main thread. gdom::Agent is not thread-safe; never call it from
// a worker thread.
//
// IPC contract with webkitengine (Go side):
//   All messages travel through the WKBundlePage postMessage channel, with the
//   message name "GWBI" (matching the Go-side ipcMagic). The message body is
//   a WKDataRef carrying raw IPC frame bytes as defined in webkitengine/ipc.go.
//
//   Frame layout (all little-endian):
//     [4]byte  magic    = "GWBI"
//     uint16   kind
//     uint16   flags    // reserved = 0
//     uint32   bodyLen
//     [bodyLen]byte body
//
//   MessageKind values (mirror webkitengine.MessageKind):
//     1 = KindDOMBatch   body = GDOM batch bytes (see protocol/protocol.go)
//     2 = KindDOMAck     body = uint64 newRevision (0 = NACK)
//     3 = KindEvent      body = uint32 kind, uint64 nodeID, uint32 valueLen, []byte value
//     4 = KindLoadHTML   body = UTF-8 HTML string
//     5 = KindMount      body = UTF-8 CSS selector
//     6 = KindMountAck   body = uint64 nodeID (0 = not found)
//
//   Go → web process: KindDOMBatch, KindLoadHTML, KindMount
//   Web process → Go: KindDOMAck, KindEvent, KindMountAck

#include <WebKit/WKBundle.h>
#include <WebKit/WKBundlePage.h>
#include <WebKit/WKBundleFrame.h>
#include <WebKit/WKBundlePageLoaderClient.h>
#include <WebKit/WKBundlePageUIClient.h>
#include <WebKit/WKData.h>
#include <WebKit/WKRetainPtr.h>
#include <WebKit/WKString.h>
#include <WebKit/WKType.h>

#include <WebCore/Document.h>
#include <WebCore/Frame.h>

#include <wtf/HashMap.h>
#include <wtf/Lock.h>
#include <wtf/NeverDestroyed.h>

#include <cstdint>
#include <cstring>

#include "dom_agent.cpp"   // gdom::Agent definition

// ---- IPC frame constants ----------------------------------------------------

static constexpr const char kIPCMagic[4] = { 'G', 'W', 'B', 'I' };
static constexpr uint16_t kKindDOMBatch  = 1;
static constexpr uint16_t kKindDOMAck    = 2;
static constexpr uint16_t kKindEvent     = 3;
static constexpr uint16_t kKindLoadHTML  = 4;
static constexpr uint16_t kKindMount     = 5;
static constexpr uint16_t kKindMountAck  = 6;

static constexpr size_t kIPCHeaderLen = 4 + 2 + 2 + 4; // 12 bytes

// Helper: read a little-endian uint16 from an unaligned pointer.
static inline uint16_t leU16(const uint8_t* p) {
    return uint16_t(p[0]) | (uint16_t(p[1]) << 8);
}
static inline uint32_t leU32(const uint8_t* p) {
    return uint32_t(p[0]) | (uint32_t(p[1]) << 8)
         | (uint32_t(p[2]) << 16) | (uint32_t(p[3]) << 24);
}
static inline uint64_t leU64(const uint8_t* p) {
    return uint64_t(leU32(p)) | (uint64_t(leU32(p + 4)) << 32);
}

// Helper: write a little-endian uint64 into an 8-byte buffer.
static inline void putLeU64(uint8_t* p, uint64_t v) {
    for (int i = 0; i < 8; ++i) { p[i] = uint8_t(v & 0xFF); v >>= 8; }
}

// ---- Per-page state ---------------------------------------------------------

// PageState bundles the DOM agent and its document for one page.
struct PageState {
    std::unique_ptr<gdom::Agent> agent;
    WKBundlePageRef page = nullptr;
    uint64_t revision = 0;
};

// Global page table keyed by WKBundlePageRef.
static Lock s_tableLock;
static HashMap<WKBundlePageRef, std::unique_ptr<PageState>>& pageTable()
{
    static NeverDestroyed<HashMap<WKBundlePageRef, std::unique_ptr<PageState>>> map;
    return map.get();
}

static PageState* stateFor(WKBundlePageRef page)
{
    Locker locker { s_tableLock };
    auto it = pageTable().find(page);
    return it == pageTable().end() ? nullptr : it->value.get();
}

// ---- Outbound helpers: broker → Go -----------------------------------------

// Post a raw IPC frame (kind + body) back to the UI-process broker.
static void postFrame(WKBundlePageRef page, uint16_t kind,
                      const uint8_t* body, uint32_t bodyLen)
{
    // Allocate: header + body.
    uint32_t total = kIPCHeaderLen + bodyLen;
    auto buf = std::make_unique<uint8_t[]>(total);

    std::memcpy(buf.get(), kIPCMagic, 4);
    buf[4] = uint8_t(kind & 0xFF);  buf[5] = uint8_t(kind >> 8);
    buf[6] = 0; buf[7] = 0;         // flags
    buf[8] = uint8_t(bodyLen);      buf[9] = uint8_t(bodyLen >> 8);
    buf[10] = uint8_t(bodyLen >> 16); buf[11] = uint8_t(bodyLen >> 24);
    if (bodyLen && body)
        std::memcpy(buf.get() + kIPCHeaderLen, body, bodyLen);

    // WKBundlePagePostMessage sends a message to the UI process broker.
    // The message name "GWBI" identifies this channel.
    auto nameRef = adoptWK(WKStringCreateWithUTF8CString("GWBI"));
    auto dataRef = adoptWK(WKDataCreate(buf.get(), total));
    WKBundlePagePostMessage(page, nameRef.get(), dataRef.get());
}

// Post KindDOMAck (uint64 newRevision, 0 = NACK).
static void postDOMAck(WKBundlePageRef page, uint64_t rev)
{
    uint8_t body[8];
    putLeU64(body, rev);
    postFrame(page, kKindDOMAck, body, 8);
}

// Post KindMountAck (uint64 nodeID, 0 = not found).
static void postMountAck(WKBundlePageRef page, uint64_t nodeID)
{
    uint8_t body[8];
    putLeU64(body, nodeID);
    postFrame(page, kKindMountAck, body, 8);
}

// ---- Event callback: DOM → Go ----------------------------------------------

// Called by gdom::Agent when a subscribed DOM event fires. Encodes and posts
// KindEvent back to the broker.
//
//   KindEvent body: uint32 kind, uint64 nodeID, uint32 valueLen, []byte value
static void onDOMEvent(WKBundlePageRef page,
                       uint32_t eventKind,
                       uint64_t nodeID,
                       const char* value,
                       uint32_t valueLen)
{
    uint32_t bodyLen = 4 + 8 + 4 + valueLen;
    auto body = std::make_unique<uint8_t[]>(bodyLen);
    uint8_t* p = body.get();

    p[0] = uint8_t(eventKind);       p[1] = uint8_t(eventKind >> 8);
    p[2] = uint8_t(eventKind >> 16); p[3] = uint8_t(eventKind >> 24);
    putLeU64(p + 4, nodeID);
    p[12] = uint8_t(valueLen);        p[13] = uint8_t(valueLen >> 8);
    p[14] = uint8_t(valueLen >> 16);  p[15] = uint8_t(valueLen >> 24);
    if (valueLen && value)
        std::memcpy(p + 16, value, valueLen);

    postFrame(page, kKindEvent, body.get(), bodyLen);
}

// ---- Bundle message handler: Go → web process ------------------------------

// Dispatches an inbound IPC frame from the UI-process broker.
// Runs on the web-process main thread.
static void handleBrokerMessage(WKBundlePageRef page,
                                const uint8_t* buf, size_t len)
{
    if (len < kIPCHeaderLen)
        return;
    if (std::memcmp(buf, kIPCMagic, 4) != 0)
        return;

    uint16_t kind    = leU16(buf + 4);
    // flags (buf+6) — ignored (reserved = 0)
    uint32_t bodyLen = leU32(buf + 8);
    if (len < kIPCHeaderLen + bodyLen)
        return;

    const uint8_t* body = buf + kIPCHeaderLen;
    PageState* state = stateFor(page);

    switch (kind) {

    case kKindLoadHTML: {
        // The broker is loading a new document. The agent will be recreated in
        // the didFinishDocumentLoad callback. Nothing to do here.
        (void)body; (void)bodyLen;
        break;
    }

    case kKindMount: {
        // Selector arrives as a UTF-8 string. Resolve it to a WebCore element,
        // bind it as the root in the agent, and reply with its NodeID.
        if (!state || !state->agent) {
            postMountAck(page, 0);
            return;
        }
        String selector = String::fromUTF8(reinterpret_cast<const char*>(body), bodyLen);
        uint64_t nodeID = state->agent->mount(selector);
        postMountAck(page, nodeID);
        break;
    }

    case kKindDOMBatch: {
        // Forward the GDOM batch to the DOM agent; reply with the new revision.
        if (!state || !state->agent) {
            postDOMAck(page, 0); // NACK: no agent yet
            return;
        }
        uint64_t newRev = state->agent->apply(body, bodyLen, state->revision);
        if (newRev != 0)
            state->revision = newRev;
        postDOMAck(page, newRev);
        break;
    }

    default:
        break; // unknown kind; ignore (forward compat)
    }
}

// ---- Page loader client callbacks ------------------------------------------

static void didFinishDocumentLoad(WKBundlePageRef page,
                                  WKBundleFrameRef frame,
                                  WKTypeRef /*userData*/,
                                  const void* /*clientInfo*/)
{
    // Only handle the main frame.
    if (WKBundleFrameIsMainFrame(frame) == false)
        return;

    // Obtain the WebCore::Document from the main frame.
    // WKBundleFrameGetJavaScriptContext is available but requires JSC; instead
    // we use the WebKit-internal WebCore::Frame accessor available to the bundle.
    WebCore::Frame* coreFrame = toImpl(frame)->coreLocalFrame();
    if (!coreFrame)
        return;
    WebCore::Document* doc = coreFrame->document();
    if (!doc)
        return;

    // (Re)create the agent for this page.
    Locker locker { s_tableLock };
    auto& entry = pageTable().ensure(page, [] { return std::make_unique<PageState>(); }).iterator->value;
    entry->page = page;
    entry->revision = 0;
    entry->agent = std::make_unique<gdom::Agent>(
        *doc,
        // Event callback closure capturing page for IPC reply.
        [page](uint32_t kind, uint64_t nodeID, const char* val, uint32_t vLen) {
            onDOMEvent(page, kind, nodeID, val, vLen);
        });
}

// ---- WKBundleClient: didCreatePage -----------------------------------------

static void didCreatePage(WKBundleRef /*bundle*/,
                          WKBundlePageRef page,
                          const void* /*clientInfo*/)
{
    // Register the page-load client so we can capture the Document on load.
    WKBundlePageLoaderClientV0 loaderClient = {};
    loaderClient.base.version = 0;
    loaderClient.didFinishDocumentLoadForFrame = didFinishDocumentLoad;
    WKBundlePageSetPageLoaderClient(page, &loaderClient.base);

    // Register the message handler that receives IPC frames from the broker.
    // This is the WKBundlePageUIClientV* postMessage mechanism: the broker
    // (Go side) calls WKPagePostMessageToInjectedBundle("GWBI", data), and the
    // web process calls this callback on the main thread.
    //
    // NOTE: WKBundlePageSetUIClient is used here; the actual callback depends
    // on the WinCairo WebKit API version. Adjust the version field as needed.
    WKBundlePageUIClientV0 uiClient = {};
    uiClient.base.version = 0;
    uiClient.willAddMessageToConsole = nullptr; // unused
    // WKBundlePage does not expose a direct postMessage receiver in the public
    // SPI in all versions. In the WinCairo fork used by this project, patch in
    // a WKBundlePageMessageHandler that calls handleBrokerMessage(page, bytes, len).
    // The hook point is: WKBundleSetShouldPostMessageToPage / WKBundlePagePostMessage
    // in the UI process, received here via WKBundleClient.didReceiveMessage.
    (void)uiClient; // placeholder — see WKBundleClient.didReceiveMessageToPage below
    WKBundlePageSetUIClient(page, &uiClient.base);
}

// didReceiveMessageToPage is called on the web-process main thread when the
// UI-process broker posts a message to a specific page via
// WKPagePostMessageToInjectedBundle.
static void didReceiveMessageToPage(WKBundleRef /*bundle*/,
                                    WKBundlePageRef page,
                                    WKStringRef messageName,
                                    WKTypeRef messageBody,
                                    const void* /*clientInfo*/)
{
    // Filter by our channel name "GWBI".
    auto name = WKStringCopyCFString(kCFAllocatorDefault, messageName); // WinCairo variant
    // Simpler: compare WKStringIsEqualToUTF8CString.
    if (!WKStringIsEqualToUTF8CString(messageName, "GWBI"))
        return;

    if (WKGetTypeID(messageBody) != WKDataGetTypeID())
        return;

    WKDataRef data = static_cast<WKDataRef>(messageBody);
    const uint8_t* bytes = WKDataGetBytes(data);
    size_t len = WKDataGetSize(data);
    handleBrokerMessage(page, bytes, len);
}

// ---- WKBundleInitialize entry point ----------------------------------------

extern "C"
void WKBundleInitialize(WKBundleRef bundle, WKTypeRef /*initData*/)
{
    WKBundleClientV1 client = {};
    client.base.version = 1;
    client.didCreatePage             = didCreatePage;
    client.didReceiveMessageToPage   = didReceiveMessageToPage;
    WKBundleSetClient(bundle, &client.base);
}
