package tools

import (
	"context"
	"fmt"
	"os"

	hccommon "github.com/lfedgeai/spear/spearlet/hostcalls/common"
	log "github.com/sirupsen/logrus"

	"github.com/chromedp/chromedp"
	"github.com/chromedp/chromedp/kb"
)

var webTools = []hccommon.ToolRegistry{
	{
		ToolType:    hccommon.ToolType_Builtin,
		Name:        "open_url",
		Id:          hccommon.BuiltinToolID_OpenURL,
		Description: `Open a URL in the default browser`,
		Params: map[string]hccommon.ToolParam{
			"url": {
				Ptype:       "string",
				Description: "URL to open",
				Required:    true,
			},
		},
		CbBuiltIn: openUrl,
	},
	{
		ToolType:    hccommon.ToolType_Builtin,
		Name:        "scroll_down",
		Id:          hccommon.BuiltinToolID_ScrollDown,
		Description: `Scroll down the page using arrowdown key`,
		Params: map[string]hccommon.ToolParam{
			"times": {
				Ptype:       "integer",
				Description: "Number of times to press arrowdown key",
				Required:    true,
			},
		},
		CbBuiltIn: scrollDown,
	},
	{
		ToolType:    hccommon.ToolType_Builtin,
		Name:        "scroll_up",
		Id:          hccommon.BuiltinToolID_ScrollUp,
		Description: `Scroll up the page using arrowup key`,
		Params: map[string]hccommon.ToolParam{
			"times": {
				Ptype:       "integer",
				Description: "Number of times to press arrowup key",
				Required:    true,
			},
		},
		CbBuiltIn: scrollUp,
	},
	{
		ToolType:    hccommon.ToolType_Builtin,
		Name:        "page_up",
		Id:          hccommon.BuiltinToolID_PageUp,
		Description: `Scroll up the page using pageup key`,
		Params:      map[string]hccommon.ToolParam{},
		CbBuiltIn:   pageUp,
	},
	{
		ToolType:    hccommon.ToolType_Builtin,
		Name:        "page_down",
		Id:          hccommon.BuiltinToolID_PageDown,
		Description: `Scroll down the page using pagedown key`,
		Params:      map[string]hccommon.ToolParam{},
		CbBuiltIn:   pageDown,
	},
	{
		ToolType:    hccommon.ToolType_Builtin,
		Name:        "web_screenshot",
		Id:          hccommon.BuiltinToolID_WebScreenshot,
		Description: `Take a screenshot of the current web page. This won't take a screenshot of the entire screen`,
		Params:      map[string]hccommon.ToolParam{},
		CbBuiltIn:   webScreenshot,
	},
}

var gCtx context.Context
var gCtxCancel context.CancelFunc
var started bool = false

func webScreenshot(inv *hccommon.InvocationInfo, args interface{}) (interface{}, error) {
	if !started {
		startChrome()
	}
	var buf []byte
	err := chromedp.Run(gCtx, chromedp.CaptureScreenshot(&buf))
	if err != nil {
		return nil, err
	}
	filename := "screenshot.png"
	// dump the screenshot to a file
	file, err := os.Create(filename)
	if err != nil {
		return nil, err
	}
	defer file.Close()
	_, err = file.Write(buf)
	if err != nil {
		return nil, err
	}

	return "Screenshot taken successfully", nil
}

func pageUp(inv *hccommon.InvocationInfo, args interface{}) (interface{}, error) {
	if !started {
		startChrome()
	}
	err := chromedp.Run(gCtx, chromedp.KeyEvent(kb.PageUp))
	if err != nil {
		return nil, err
	}
	return "Scrolled up one page", nil
}

func pageDown(inv *hccommon.InvocationInfo, args interface{}) (interface{}, error) {
	if !started {
		startChrome()
	}
	err := chromedp.Run(gCtx, chromedp.KeyEvent(kb.PageDown))
	if err != nil {
		return nil, err
	}
	return "Scrolled down one page", nil
}

func scrollDown(inv *hccommon.InvocationInfo, args interface{}) (interface{}, error) {
	if !started {
		startChrome()
	}
	// convert the args from float64 to int
	times := int(args.(map[string]interface{})["times"].(float64))
	log.Infof("Scrolling down %d times", times)
	for i := 0; i < times; i++ {
		err := chromedp.Run(gCtx, chromedp.KeyEvent(kb.ArrowDown))
		if err != nil {
			return nil, err
		}
	}
	return fmt.Sprintf("Scrolled down %d times", times), nil
}

func scrollUp(inv *hccommon.InvocationInfo, args interface{}) (interface{}, error) {
	if !started {
		startChrome()
	}
	// convert the args from float64 to int
	times := int(args.(map[string]interface{})["times"].(float64))
	log.Infof("Scrolling up %d times", times)
	for i := 0; i < times; i++ {
		err := chromedp.Run(gCtx, chromedp.KeyEvent(kb.ArrowUp))
		if err != nil {
			return nil, err
		}
	}
	return fmt.Sprintf("Scrolled up %d times", times), nil
}

func openUrl(inv *hccommon.InvocationInfo, args interface{}) (interface{}, error) {
	if !started {
		startChrome()
	}
	url := args.(map[string]interface{})["url"].(string)
	err := chromedp.Run(gCtx, chromedp.Navigate(url))
	if err != nil {
		return nil, err
	}
	return fmt.Sprintf("URL %s opened successfully", url), nil
}

func startChrome() bool {
	if started {
		return false
	}
	// use chromedp to open URL
	opts := append(chromedp.DefaultExecAllocatorOptions[:],
		chromedp.Flag("headless", false),
	)
	ctx, _ := chromedp.NewExecAllocator(context.Background(), opts...)
	gCtx, gCtxCancel = chromedp.NewContext(ctx)
	started = true
	return true
}

func init() {
	for _, tool := range webTools {
		hccommon.RegisterBuiltinTool(tool)
	}
}
