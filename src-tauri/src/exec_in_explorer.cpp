// THIS CODE AND INFORMATION IS PROVIDED "AS IS" WITHOUT WARRANTY OF
// ANY KIND, EITHER EXPRESSED OR IMPLIED, INCLUDING BUT NOT LIMITED TO
// THE IMPLIED WARRANTIES OF MERCHANTABILITY AND/OR FITNESS FOR A
// PARTICULAR PURPOSE.
//
// Copyright (c) Microsoft Corporation. All rights reserved.
// Adapted from Microsoft's ExecInExplorer Windows classic sample.

#include <windows.h>
#include <shlwapi.h>
#include <shlobj.h>
#include <shldisp.h>

#pragma comment(lib, "ole32.lib")
#pragma comment(lib, "oleaut32.lib")
#pragma comment(lib, "shlwapi.lib")

static HRESULT GetShellViewForDesktop(REFIID riid, void **ppv)
{
    *ppv = nullptr;
    IShellWindows *shellWindows = nullptr;
    HRESULT result = CoCreateInstance(
        CLSID_ShellWindows,
        nullptr,
        CLSCTX_LOCAL_SERVER,
        IID_PPV_ARGS(&shellWindows));
    if (FAILED(result)) {
        return result;
    }

    HWND window = nullptr;
    IDispatch *dispatch = nullptr;
    VARIANT empty = {};
    if (S_OK != shellWindows->FindWindowSW(
                    &empty,
                    &empty,
                    SWC_DESKTOP,
                    reinterpret_cast<long *>(&window),
                    SWFO_NEEDDISPATCH,
                    &dispatch)) {
        shellWindows->Release();
        return E_FAIL;
    }

    IShellBrowser *browser = nullptr;
    result = IUnknown_QueryService(
        dispatch,
        SID_STopLevelBrowser,
        IID_PPV_ARGS(&browser));
    if (SUCCEEDED(result)) {
        IShellView *view = nullptr;
        result = browser->QueryActiveShellView(&view);
        if (SUCCEEDED(result)) {
            result = view->QueryInterface(riid, ppv);
            view->Release();
        }
        browser->Release();
    }
    dispatch->Release();
    shellWindows->Release();
    return result;
}

static HRESULT GetShellDispatchFromView(IShellView *view, REFIID riid, void **ppv)
{
    *ppv = nullptr;
    IDispatch *background = nullptr;
    HRESULT result = view->GetItemObject(SVGIO_BACKGROUND, IID_PPV_ARGS(&background));
    if (FAILED(result)) {
        return result;
    }

    IShellFolderViewDual *folderView = nullptr;
    result = background->QueryInterface(IID_PPV_ARGS(&folderView));
    if (SUCCEEDED(result)) {
        IDispatch *application = nullptr;
        result = folderView->get_Application(&application);
        if (SUCCEEDED(result)) {
            result = application->QueryInterface(riid, ppv);
            application->Release();
        }
        folderView->Release();
    }
    background->Release();
    return result;
}

extern "C" HRESULT rbx_shell_execute_unelevated(
    PCWSTR file,
    PCWSTR arguments,
    PCWSTR directory)
{
    HRESULT initialized = CoInitializeEx(
        nullptr,
        COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
    if (FAILED(initialized)) {
        return initialized;
    }

    IShellView *view = nullptr;
    HRESULT result = GetShellViewForDesktop(IID_PPV_ARGS(&view));
    if (SUCCEEDED(result)) {
        IShellDispatch2 *shell = nullptr;
        result = GetShellDispatchFromView(view, IID_PPV_ARGS(&shell));
        if (SUCCEEDED(result)) {
            BSTR fileValue = SysAllocString(file);
            BSTR argumentValue = SysAllocString(arguments);
            BSTR directoryValue = SysAllocString(directory);
            if (!fileValue || !argumentValue || !directoryValue) {
                result = E_OUTOFMEMORY;
            } else {
                VARIANT argumentVariant = {};
                argumentVariant.vt = VT_BSTR;
                argumentVariant.bstrVal = argumentValue;
                VARIANT directoryVariant = {};
                directoryVariant.vt = VT_BSTR;
                directoryVariant.bstrVal = directoryValue;
                VARIANT operation = {};
                operation.vt = VT_BSTR;
                operation.bstrVal = SysAllocString(L"open");
                VARIANT show = {};
                show.vt = VT_I4;
                show.lVal = SW_SHOWNORMAL;
                if (!operation.bstrVal) {
                    result = E_OUTOFMEMORY;
                } else {
                    result = shell->ShellExecute(
                        fileValue,
                        argumentVariant,
                        directoryVariant,
                        operation,
                        show);
                }
                VariantClear(&operation);
            }
            SysFreeString(fileValue);
            SysFreeString(argumentValue);
            SysFreeString(directoryValue);
            shell->Release();
        }
        view->Release();
    }
    CoUninitialize();
    return result;
}
